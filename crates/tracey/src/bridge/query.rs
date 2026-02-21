//! Shared query formatter helpers for MCP and CLI query commands.
//!
//! This module contains the actual query-to-client formatting logic so both
//! MCP and terminal queries print the same markdown-like output.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::daemon::{DaemonClient, new_client};
use tracey_core::parse_rule_id;
use tracey_proto::*;

/// Convert roam RPC result to a simple Result
fn rpc<T, E: std::fmt::Debug>(res: Result<T, roam_stream::CallError<E>>) -> Result<T, String> {
    res.map_err(|e| format!("RPC error: {:?}", e))
}

/// Format config error as a warning banner to prepend to responses
fn format_config_error_banner(error: &str) -> String {
    format!(
        "⚠️  CONFIG ERROR ⚠️\n{}\n\nFix the config file and the daemon will automatically reload.\n\n---\n\n",
        error
    )
}

/// Parse "spec/impl" format into `(Option<spec>, Option<impl>)`.
fn parse_spec_impl(spec_impl: Option<&str>) -> (Option<String>, Option<String>) {
    match spec_impl {
        Some(s) if s.contains('/') => {
            let parts: Vec<&str> = s.splitn(2, '/').collect();
            (Some(parts[0].to_string()), Some(parts[1].to_string()))
        }
        Some(s) => (Some(s.to_string()), None),
        None => (None, None),
    }
}

/// Shared query client used by both MCP and CLI.
#[derive(Clone)]
pub struct QueryClient {
    client: Arc<Mutex<DaemonClient>>,
}

impl QueryClient {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            client: Arc::new(Mutex::new(new_client(project_root))),
        }
    }

    /// Check for config errors and return a warning banner if present.
    async fn get_config_error_banner(&self) -> Option<String> {
        let client = self.client.lock().await;
        match rpc(client.health().await) {
            Ok(health) => health.config_error.map(|e| format_config_error_banner(&e)),
            Err(_) => None,
        }
    }

    pub async fn with_config_banner(&self, output: String) -> String {
        if let Some(banner) = self.get_config_error_banner().await {
            format!("{}{}", banner, output)
        } else {
            output
        }
    }

    /// Get coverage status for all specs/implementations
    pub async fn status(&self) -> String {
        let client = self.client.lock().await;
        let output = match rpc(client.status().await) {
            Ok(status) => {
                let mut output = String::new();
                for impl_status in &status.impls {
                    let impl_pct = if impl_status.total_rules > 0 {
                        impl_status.covered_rules as f64 / impl_status.total_rules as f64 * 100.0
                    } else {
                        0.0
                    };
                    let verify_pct = if impl_status.total_rules > 0 {
                        impl_status.verified_rules as f64 / impl_status.total_rules as f64 * 100.0
                    } else {
                        0.0
                    };
                    output.push_str(&format!(
                        "{}/{}: impl {:.0}%, verify {:.0}% ({}/{} rules)\n",
                        impl_status.spec,
                        impl_status.impl_name,
                        impl_pct,
                        verify_pct,
                        impl_status.covered_rules,
                        impl_status.total_rules
                    ));
                }

                if output.is_empty() {
                    "No specs configured".to_string()
                } else {
                    output.push_str("\n---\n");
                    output.push_str("→ Use tracey_uncovered to see rules without implementation\n");
                    output.push_str("→ Use tracey_untested to see rules without verification\n");
                    output.push_str("→ Use tracey_unmapped to see code without requirements\n");
                    output
                }
            }
            Err(e) => format!("Error: {}", e),
        };

        self.with_config_banner(output).await
    }

    /// Get rules without implementation references
    pub async fn uncovered(&self, spec_impl: Option<&str>, prefix: Option<&str>) -> String {
        let client = self.client.lock().await;
        let (spec, impl_name) = parse_spec_impl(spec_impl);

        let req = UncoveredRequest {
            spec,
            impl_name,
            prefix: prefix.map(String::from),
        };

        let output = match rpc(client.uncovered(req).await) {
            Ok(response) => {
                let mut output = format!(
                    "{}/{}: {} uncovered out of {} rules\n\n",
                    response.spec,
                    response.impl_name,
                    response.uncovered_count,
                    response.total_rules
                );

                for section in &response.by_section {
                    if !section.rules.is_empty() {
                        output.push_str(&format!("## {}\n", section.section));
                        for rule in &section.rules {
                            output.push_str(&format!("  - {}\n", rule.id));
                        }
                        output.push('\n');
                    }
                }

                output.push_str("---\n");
                output.push_str("→ Use tracey_rule to see details about a specific rule\n");
                output.push_str("→ Use prefix parameter to filter by rule ID prefix\n");

                output
            }
            Err(e) => format!("Error: {}", e),
        };

        self.with_config_banner(output).await
    }

    /// Get rules without verification references
    pub async fn untested(&self, spec_impl: Option<&str>, prefix: Option<&str>) -> String {
        let client = self.client.lock().await;
        let (spec, impl_name) = parse_spec_impl(spec_impl);

        let req = UntestedRequest {
            spec,
            impl_name,
            prefix: prefix.map(String::from),
        };

        let output = match rpc(client.untested(req).await) {
            Ok(response) => {
                let mut output = format!(
                    "{}/{}: {} untested (impl but no verify) out of {} rules\n\n",
                    response.spec,
                    response.impl_name,
                    response.untested_count,
                    response.total_rules
                );

                for section in &response.by_section {
                    if !section.rules.is_empty() {
                        output.push_str(&format!("## {}\n", section.section));
                        for rule in &section.rules {
                            output.push_str(&format!("  - {}\n", rule.id));
                        }
                        output.push('\n');
                    }
                }

                output.push_str("---\n");
                output.push_str("→ Use tracey_rule to see details about a specific rule\n");
                output.push_str("→ Use prefix parameter to filter by rule ID prefix\n");

                output
            }
            Err(e) => format!("Error: {}", e),
        };

        self.with_config_banner(output).await
    }

    /// Get code units without rule references
    pub async fn unmapped(&self, spec_impl: Option<&str>, path: Option<&str>) -> String {
        let client = self.client.lock().await;
        let (spec, impl_name) = parse_spec_impl(spec_impl);

        let req = UnmappedRequest {
            spec,
            impl_name,
            path: path.map(String::from),
        };

        let output = match rpc(client.unmapped(req).await) {
            Ok(response) => {
                let mut output = format!(
                    "{}/{}: {} unmapped code units out of {} total\n\n",
                    response.spec,
                    response.impl_name,
                    response.unmapped_count,
                    response.total_units
                );

                // Check if we're zoomed into a file with unit details
                let has_unit_details = response.entries.iter().any(|e| !e.units.is_empty());

                if has_unit_details {
                    // File zoom view - show unmapped code units with line numbers
                    for entry in &response.entries {
                        if !entry.units.is_empty() {
                            output.push_str(&format!("## {}\n\n", entry.path));
                            for unit in &entry.units {
                                let name = unit.name.as_deref().unwrap_or("<anonymous>");
                                output.push_str(&format!(
                                    "  L{}-{}: {} `{}`\n",
                                    unit.start_line, unit.end_line, unit.kind, name
                                ));
                            }
                            output.push('\n');
                        }
                    }
                } else {
                    // Tree view - format as ASCII tree with progress bars
                    for (i, entry) in response.entries.iter().enumerate() {
                        let pct = if entry.total_units > 0 {
                            (entry.total_units - entry.unmapped_units) as f64
                                / entry.total_units as f64
                                * 100.0
                        } else {
                            100.0
                        };

                        // Progress bar (10 chars)
                        let filled = (pct / 10.0).round() as usize;
                        let bar: String = "█".repeat(filled) + &"░".repeat(10 - filled);

                        // Tree connector
                        let is_last = i == response.entries.len() - 1;
                        let connector = if is_last { "└── " } else { "├── " };

                        output.push_str(&format!(
                            "{}{:<30} {:>3.0}% {}\n",
                            connector, entry.path, pct, bar
                        ));
                    }
                }

                output.push_str("\n---\n");
                output.push_str("→ Use path parameter to zoom into a directory or file\n");

                output
            }
            Err(e) => format!("Error: {}", e),
        };

        self.with_config_banner(output).await
    }

    pub async fn rule(&self, rule_id: &str) -> String {
        let client = self.client.lock().await;
        let Some(rule_id) = parse_rule_id(rule_id) else {
            return "Error: invalid rule ID".to_string();
        };

        let output = match rpc(client.rule(rule_id.clone()).await) {
            Ok(Some(info)) => {
                let mut output = format!("# {}\n\n{}\n\n", info.id, info.raw);

                if let Some(file) = &info.source_file
                    && let Some(line) = info.source_line
                {
                    output.push_str(&format!("Defined in: {}:{}\n\n", file, line));
                }

                for cov in &info.coverage {
                    output.push_str(&format!("\n## {}/{}\n", cov.spec, cov.impl_name));
                    if !cov.impl_refs.is_empty() {
                        output.push_str("Impl references:\n");
                        for r in &cov.impl_refs {
                            output.push_str(&format!("  - {}:{}\n", r.file, r.line));
                        }
                    }
                    if !cov.verify_refs.is_empty() {
                        output.push_str("Verify references:\n");
                        for r in &cov.verify_refs {
                            output.push_str(&format!("  - {}:{}\n", r.file, r.line));
                        }
                    }
                }

                output
            }
            Ok(None) => format!("Rule not found: {}", rule_id),
            Err(e) => format!("Error: {}", e),
        };

        self.with_config_banner(output).await
    }

    /// Display current configuration
    pub async fn config(&self) -> String {
        let client = self.client.lock().await;
        let output = match rpc(client.config().await) {
            Ok(config) => {
                let mut output = String::from("# Tracey Configuration\n\n");

                for spec in &config.specs {
                    output.push_str(&format!("## Spec: {}\n", spec.name));
                    output.push_str(&format!("  Prefix: {}\n", spec.prefix));
                    if let Some(source) = &spec.source {
                        output.push_str(&format!("  Source: {}\n", source));
                    }
                    output.push_str(&format!(
                        "  Implementations: {}\n\n",
                        spec.implementations.join(", ")
                    ));
                }

                output
            }
            Err(e) => format!("Error: {}", e),
        };

        self.with_config_banner(output).await
    }

    pub async fn reload(&self) -> String {
        let client = self.client.lock().await;
        let output = match rpc(client.reload().await) {
            Ok(response) => format!(
                "Reload complete (version {}, took {}ms)",
                response.version, response.rebuild_time_ms
            ),
            Err(e) => format!("Error: {}", e),
        };

        self.with_config_banner(output).await
    }

    pub async fn validate(&self, spec_impl: Option<&str>) -> String {
        let client = self.client.lock().await;
        let output = if spec_impl.is_some() {
            // If a specific spec/impl was requested, validate just that one.
            let (spec, impl_name) = parse_spec_impl(spec_impl);
            let req = ValidateRequest { spec, impl_name };
            match rpc(client.validate(req).await) {
                Ok(result) => format_validation_result(&result),
                Err(e) => format!("Error: {}", e),
            }
        } else {
            // No filter provided: validate ALL spec/impl combinations.
            let status = match rpc(client.status().await) {
                Ok(s) => s,
                Err(e) => {
                    return self
                        .with_config_banner(format!("Error getting status: {}", e))
                        .await;
                }
            };

            if status.impls.is_empty() {
                "No spec/impl combinations configured.".to_string()
            } else {
                let mut output = String::new();
                let mut total_errors = 0;

                for impl_status in &status.impls {
                    let req = ValidateRequest {
                        spec: Some(impl_status.spec.clone()),
                        impl_name: Some(impl_status.impl_name.clone()),
                    };

                    match rpc(client.validate(req).await) {
                        Ok(result) => {
                            total_errors += result.error_count;
                            output.push_str(&format_validation_result(&result));
                            output.push('\n');
                        }
                        Err(e) => {
                            output.push_str(&format!(
                                "✗ {}/{}: Error: {}\n\n",
                                impl_status.spec, impl_status.impl_name, e
                            ));
                        }
                    }
                }

                output.push_str("---\n");
                output.push_str(&format!(
                    "Validated {} spec/impl combination(s), {} total error(s)\n",
                    status.impls.len(),
                    total_errors
                ));
                output.push_str(
                    "→ Use spec_impl parameter to validate a specific one (e.g., \"my-spec/rust\")\n",
                );
                output
            }
        };

        self.with_config_banner(output).await
    }

    pub async fn config_exclude(&self, spec_impl: Option<&str>, pattern: &str) -> String {
        let client = self.client.lock().await;
        let (spec, impl_name) = parse_spec_impl(spec_impl);

        let req = ConfigPatternRequest {
            spec,
            impl_name,
            pattern: pattern.to_string(),
        };

        let output = match rpc(client.config_add_exclude(req).await) {
            Ok(()) => format!("Added exclude pattern: {}", pattern),
            Err(e) => format!("Error: {}", e),
        };

        self.with_config_banner(output).await
    }

    pub async fn config_include(&self, spec_impl: Option<&str>, pattern: &str) -> String {
        let client = self.client.lock().await;
        let (spec, impl_name) = parse_spec_impl(spec_impl);

        let req = ConfigPatternRequest {
            spec,
            impl_name,
            pattern: pattern.to_string(),
        };

        let output = match rpc(client.config_add_include(req).await) {
            Ok(()) => format!("Added include pattern: {}", pattern),
            Err(e) => format!("Error: {}", e),
        };

        self.with_config_banner(output).await
    }
}

/// Format a validation result for display.
fn format_validation_result(result: &tracey_proto::ValidationResult) -> String {
    if result.errors.is_empty() {
        format!(
            "✓ {}/{}: No validation errors found",
            result.spec, result.impl_name
        )
    } else {
        let mut output = format!(
            "✗ {}/{}: {} error(s) found\n",
            result.spec, result.impl_name, result.error_count
        );

        for error in &result.errors {
            let location = match (&error.file, error.line) {
                (Some(f), Some(l)) => format!(" at {}:{}", f, l),
                (Some(f), None) => format!(" in {}", f),
                _ => String::new(),
            };

            if error.code == ValidationErrorCode::StaleRequirement {
                // r[impl mcp.validation.stale.message-prefix]
                output.push_str(&format!(
                    "  - {} [{:?}]{}\n",
                    error.message, error.code, location
                ));
            } else {
                output.push_str(&format!(
                    "  - [{:?}] {}{}\n",
                    error.code, error.message, location
                ));
            }

            if !error.related_rules.is_empty() {
                output.push_str(&format!(
                    "    Related rules: {}\n",
                    error
                        .related_rules
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::format_validation_result;
    use tracey_core::parse_rule_id;
    use tracey_proto::{ValidationError, ValidationErrorCode, ValidationResult};

    #[test]
    fn stale_validation_output_starts_with_message_text() {
        let result = ValidationResult {
            spec: "spec".to_string(),
            impl_name: "impl".to_string(),
            errors: vec![ValidationError {
                code: ValidationErrorCode::StaleRequirement,
                message: "Implementation must be changed to match updated rule text — and ONLY ONCE THAT'S DONE must the code annotation be bumped. Example".to_string(),
                file: Some("src/lib.rs".to_string()),
                line: Some(12),
                column: None,
                related_rules: vec![parse_rule_id("spec.rule+2").expect("valid rule id")],
            }],
            warning_count: 0,
            error_count: 1,
        };

        let output = format_validation_result(&result);
        assert!(
            output.contains(
                "  - Implementation must be changed to match updated rule text — and ONLY ONCE THAT'S DONE must the code annotation be bumped."
            ),
            "unexpected output:\n{}",
            output
        );
    }
}
