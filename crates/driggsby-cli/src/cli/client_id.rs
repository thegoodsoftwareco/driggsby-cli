pub(super) fn canonicalize(input: &str) -> String {
    input.trim().to_ascii_lowercase()
}

pub(super) fn is_valid(client_id: &str) -> bool {
    !client_id.is_empty()
        && client_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}
