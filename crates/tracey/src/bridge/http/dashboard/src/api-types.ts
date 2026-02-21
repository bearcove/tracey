// This file is auto-generated from tracey-api Rust types
// DO NOT EDIT MANUALLY - changes will be overwritten on build

/**
 * r[impl validation.circular-deps]
 * r[impl validation.naming]
 *
 * A validation error found in the spec or implementation.
 */
export interface ValidationError {
  /**
   * Error code for programmatic handling
   */
  code: ValidationErrorCode;
  /**
   * Human-readable error message
   */
  message: string;
  /**
   * File where the error was found (if applicable)
   */
  file?: string;
  /**
   * Line number (if applicable)
   */
  line?: number;
  /**
   * Column number (if applicable)
   */
  column?: number;
  /**
   * Related rule IDs (for dependency errors)
   */
  relatedRules?: RuleId[];
}

/**
 * Structured rule ID representation.
 */
export interface RuleId {
  /**
   * Base rule ID without version suffix.
   */
  base: string;
  /**
   * Normalized version number (unversioned IDs are version 1).
   */
  version: number;
}

/**
 * Error codes for validation errors
 */
export type ValidationErrorCode = "circular_dependency" | "invalid_naming" | "unknown_requirement" | "stale_requirement" | "duplicate_requirement" | "unknown_prefix" | "impl_in_test_file";

/**
 * Validation results for a spec/implementation pair
 */
export interface ValidationResult {
  /**
   * Spec name
   */
  spec: string;
  /**
   * Implementation name
   */
  implName: string;
  /**
   * List of validation errors found
   */
  errors: ValidationError[];
  /**
   * Number of warnings (non-fatal issues)
   */
  warningCount: number;
  /**
   * Number of errors (fatal issues)
   */
  errorCount: number;
}

/**
 * Spec content (may span multiple files)
 */
export interface ApiSpecData {
  name: string;
  /**
   * Sections ordered by weight
   */
  sections: SpecSection[];
  /**
   * Outline with coverage info
   */
  outline: OutlineEntry[];
}

/**
 * An entry in the spec outline (heading with coverage info)
 */
export interface OutlineEntry {
  /**
   * Heading text
   */
  title: string;
  /**
   * Slug for linking
   */
  slug: string;
  /**
   * Heading level (1-6)
   */
  level: number;
  /**
   * Direct coverage (rules directly under this heading)
   */
  coverage: OutlineCoverage;
  /**
   * Aggregated coverage (includes all nested rules)
   */
  aggregated: OutlineCoverage;
}

/**
 * Coverage counts for an outline entry
 */
export interface OutlineCoverage {
  /**
   * Number of rules with implementation refs
   */
  implCount: number;
  /**
   * Number of rules with verification refs
   */
  verifyCount: number;
  /**
   * Total number of rules
   */
  total: number;
}

/**
 * A section of a spec (one source file)
 */
export interface SpecSection {
  /**
   * Source file path
   */
  sourceFile: string;
  /**
   * Rendered HTML content
   */
  html: string;
  /**
   * Weight for ordering (from frontmatter)
   */
  weight: number;
}

export interface ApiCodeUnit {
  kind: string;
  name?: string;
  startLine: number;
  endLine: number;
  /**
   * Rule references found in this code unit's comments
   */
  ruleRefs: string[];
}

/**
 * Single file with full coverage details
 */
export interface ApiFileData {
  path: string;
  content: string;
  /**
   * Syntax-highlighted HTML content
   */
  html: string;
  /**
   * Code units in this file with their coverage
   */
  units: ApiCodeUnit[];
}

export interface ApiFileEntry {
  path: string;
  /**
   * Number of code units in this file
   */
  totalUnits: number;
  /**
   * Number of covered code units
   */
  coveredUnits: number;
}

/**
 * Reverse traceability: file tree with coverage info
 */
export interface ApiReverseData {
  /**
   * Total code units across all files
   */
  totalUnits: number;
  /**
   * Code units with at least one rule reference
   */
  coveredUnits: number;
  /**
   * File tree with coverage info
   */
  files: ApiFileEntry[];
}

export interface ApiCodeRef {
  file: string;
  line: number;
}

export interface ApiRule {
  id: RuleId;
  /**
   * Raw markdown source (without r[...] marker, but with `>` prefixes for blockquote rules)
   */
  raw: string;
  /**
   * Rendered HTML (for dashboard display)
   */
  html: string;
  status?: string;
  level?: string;
  sourceFile?: string;
  sourceLine?: number;
  sourceColumn?: number;
  /**
   * Section slug (heading ID) that this rule belongs to
   */
  section?: string;
  /**
   * Section title (heading text) that this rule belongs to
   */
  sectionTitle?: string;
  implRefs: ApiCodeRef[];
  verifyRefs: ApiCodeRef[];
  dependsRefs: ApiCodeRef[];
  isStale?: boolean;
}

export interface ApiSpecForward {
  name: string;
  rules: ApiRule[];
}

/**
 * Forward traceability: rules with their code references
 */
export interface ApiForwardData {
  specs: ApiSpecForward[];
}

export interface ApiSpecInfo {
  name: string;
  /**
   * @tracey:ignore-next-line
   * Prefix used in annotations (e.g., "r" for r[req.id])
   */
  prefix: string;
  /**
   * Path to spec file(s) if local
   */
  source?: string;
  /**
   * Canonical URL for the specification (e.g., a GitHub repository)
   */
  sourceUrl?: string;
  /**
   * Available implementations for this spec
   */
  implementations: string[];
}

/**
 * Project configuration info
 */
export interface ApiConfig {
  projectRoot: string;
  specs: ApiSpecInfo[];
}

/**
 * Git status for a file
 */
export type GitStatus = "dirty" | "staged" | "clean" | "unknown";


