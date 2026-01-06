// Test file for tracey-lsp-proto
//
// Try the following in Zed with the LSP configured:
//
// 1. Hover over the requirement ID below - should show description
// 2. Go-to-definition on the requirement ID - should jump to fake spec file
// 3. Type r[ and see if completions appear

fn validate_token() {
    // r[impl auth.token.validation]
    // This implements token validation
}

fn refresh_token() {
    // r[impl auth.token.refresh]
}

fn handle_api_response() {
    // r[impl api.response.format]
}

// Try typing here:
// r[impl
// ^ after typing r[ you should get verb completions
// After typing r[impl  you should get requirement ID completions

fn test_unknown() {
    // r[impl unknown.requirement.here]
    // ^ hovering should show "Unknown requirement"
}
