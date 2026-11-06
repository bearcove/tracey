// Implementation site: @relation, no explicit role → Implements.
// @relation(BR-001, scope=function)
pub fn connect() {}

// Test site: explicit Verifies role.
// @relation(BR-002, scope=function, role=Verifies)
#[test]
fn test_heartbeat_emitted() {}

// Multi-UID annotation; both UIDs share the same span.
// @relation(BR-001, BR-002, scope=function, role=Verifies)
fn dual_verify() {}

// Refines role is rejected for v1: warning, no reference produced.
// @relation(BR-003, role=Refines)
fn refines_placeholder() {}

// Legacy r[...] marker uses the same prefix and continues to work alongside @relation.
// r[impl BR-003]
pub fn reconnect() {}

// Noun-form role and class scope (upstream StrictDoc's own docs use
// role=Implementation).
// @relation(BR-004, scope=class, role=Implementation)
pub struct Sessions;

// Range markers: range_start produces the reference, range_end closes it.
// @relation(BR-005, scope=range_start)
pub fn expire_sessions() {}
// @relation(BR-005, scope=range_end)

// role=Test maps to verify.
// @relation(BR-006, scope=function, role=Test)
fn status_page_test() {}

// A mention in prose is not a marker: see @relation(BR-006) for details.
