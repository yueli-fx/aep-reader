pub(crate) fn u16(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        d.get(o..o.checked_add(2)?)?.try_into().ok()?,
    ))
}
pub(crate) fn u32(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        d.get(o..o.checked_add(4)?)?.try_into().ok()?,
    ))
}
pub(crate) fn i32(d: &[u8], o: usize) -> Option<i32> {
    u32(d, o).map(|n| n as i32)
}
pub(crate) fn f64(d: &[u8], o: usize, little: bool) -> Option<f64> {
    let b = d.get(o..o.checked_add(8)?)?.try_into().ok()?;
    Some(if little {
        f64::from_le_bytes(b)
    } else {
        f64::from_be_bytes(b)
    })
}
pub(crate) fn f32(d: &[u8], o: usize) -> Option<f64> {
    Some(f64::from(f32::from_be_bytes(
        d.get(o..o.checked_add(4)?)?.try_into().ok()?,
    )))
}
