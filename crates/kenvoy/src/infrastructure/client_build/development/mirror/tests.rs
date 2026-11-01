use super::*;
#[test]
fn mirror_budget_is_checked_before_allocation_and_stream_reads_are_bounded() -> Result<()> {
    assert!(remaining_capacity(MAX_MIRROR_FILES, 0).is_err());
    assert!(remaining_capacity(0, MAX_MIRROR_TOTAL + 1).is_err());
    assert_eq!(remaining_capacity(0, MAX_MIRROR_TOTAL - 3)?, 3);
    assert_eq!(remaining_capacity(0, 0)?, MAX_MIRROR_FILE);
    let mut input = std::io::Cursor::new([7_u8; 64]);
    assert!(read_bounded(&mut input, 8).is_err());
    assert_eq!(input.position(), 9);
    assert_eq!(
        read_bounded(std::io::Cursor::new([7_u8; 8]), 8).map_err(|error| io("测试流", error))?,
        vec![7; 8]
    );
    Ok(())
}
