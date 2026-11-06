use super::*;
pub fn decode_calendar(code: Option<i32>, bytes: &[u8]) -> Result<RegionalCalendar, SetupFailure> {
    #[derive(Deserialize)]
    struct Data {
        work_calendar: RegionalCalendar,
    }
    let data: Data = owner_data(code, bytes)?;
    Ok(data.work_calendar)
}
