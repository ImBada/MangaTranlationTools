/// `new Date().toISOString()` format: `2026-04-30T18:58:32.492Z`.
pub fn now_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn iso_format_has_millis_and_z() {
        let value = super::now_iso();
        assert_eq!(value.len(), 24, "{value}");
        assert!(value.ends_with('Z'));
        assert_eq!(&value[19..20], ".");
    }
}
