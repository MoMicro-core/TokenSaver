use super::store::Fact;

pub fn format(facts: &[Fact]) -> String {
    if facts.is_empty() {
        return String::new();
    }
    let lines: Vec<String> = facts.iter().map(|f| format!("- {}", f.text)).collect();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::store::Fact;

    #[test]
    fn formats_facts_as_bullet_list() {
        let facts = vec![
            Fact { id: "a".into(), category: "architecture".into(), text: "Uses FastAPI".into() },
            Fact { id: "b".into(), category: "constraints".into(), text: "No schema changes".into() },
        ];
        let output = format(&facts);
        assert_eq!(output, "- Uses FastAPI\n- No schema changes");
    }

    #[test]
    fn returns_empty_for_no_facts() {
        assert_eq!(format(&[]), "");
    }
}
