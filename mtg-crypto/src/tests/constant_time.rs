use crate::constant_time_eq;

#[test]
fn compares_every_byte_and_the_length() {
    assert!(constant_time_eq(b"abc", b"abc"));
    assert!(!constant_time_eq(b"abc", b"abd"));
    assert!(!constant_time_eq(b"abc", b"xbc"));
    assert!(!constant_time_eq(b"abc", b"ab"));
}
