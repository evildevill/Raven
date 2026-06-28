use std::collections::HashSet;

pub fn generate_variants(username: &str) -> Vec<String> {
    let mut variants = HashSet::new();

    let separators = ["", "_", ".", "-"];
    for sep in &separators {
        variants.insert(username.replace('_', sep));
        variants.insert(username.replace('.', sep));
        variants.insert(username.replace('-', sep));
    }

    for suffix in &["1", "2", "01", "99", "123", "007", "x", "real", "official", "0"] {
        variants.insert(format!("{username}{suffix}"));
        variants.insert(format!("{suffix}{username}"));
    }

    for prefix in &["the", "its", "im", "i_am", "real"] {
        variants.insert(format!("{prefix}{username}"));
        variants.insert(format!("{prefix}_{username}"));
    }

    let leet = username
        .to_lowercase()
        .replace('a', "4")
        .replace('e', "3")
        .replace('i', "1")
        .replace('o', "0")
        .replace('s', "5")
        .replace('t', "7");
    variants.insert(leet);

    variants.insert(username.to_lowercase());
    variants.insert(username.to_uppercase());

    let no_numbers: String = username.chars().filter(|c| !c.is_ascii_digit()).collect();
    if !no_numbers.is_empty() && no_numbers != username {
        variants.insert(no_numbers);
    }

    variants.remove(username);
    variants.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_separator_variants() {
        let v = generate_variants("john_doe");
        assert!(v.contains(&"johndoe".to_string()));
        assert!(v.contains(&"john.doe".to_string()));
        assert!(v.contains(&"john-doe".to_string()));
    }

    #[test]
    fn generates_suffix_variants() {
        let v = generate_variants("johndoe");
        assert!(v.contains(&"johndoe1".to_string()));
        assert!(v.contains(&"johndoe99".to_string()));
    }

    #[test]
    fn generates_leet() {
        let v = generate_variants("elite");
        // e→3, l→l, i→1, t→7, e→3
        assert!(v.contains(&"3l173".to_string()), "leet 'elite' should produce '3l173', got: {v:?}");
    }

    #[test]
    fn removes_original() {
        let v = generate_variants("johndoe");
        assert!(!v.contains(&"johndoe".to_string()));
    }

    #[test]
    fn generates_more_than_10_variants() {
        let v = generate_variants("test");
        assert!(v.len() > 10, "got {} variants", v.len());
    }
}
