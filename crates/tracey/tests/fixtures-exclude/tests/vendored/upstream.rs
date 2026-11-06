// Third-party code under a test directory. It is excluded, so neither the
// `impl` marker (forbidden in test files) nor the unknown requirement may
// produce validation errors.
// r[impl auth.login]
// r[impl not.in.spec]
pub fn vendored() {}
