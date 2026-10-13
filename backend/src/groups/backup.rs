// @created 2026-10-13
// @description 校验备份中的分组并生成独立的新 ID，保持顺序与成员关联。
// @author yunhungo
use super::{name, Group, MAX_GROUPS};
use crate::error::ApiError;
use std::collections::{BTreeMap, HashSet};

pub fn prepare(mut groups: Vec<Group>) -> Result<(Vec<Group>, BTreeMap<String, String>), ApiError> {
    if groups.len() > MAX_GROUPS {
        return Err(ApiError::bad("备份分组数量超过上限"));
    }
    groups.sort_by(|a, b| (a.position, &a.id).cmp(&(b.position, &b.id)));
    let mut names = HashSet::new();
    let mut ids = BTreeMap::new();
    for (position, group) in groups.iter_mut().enumerate() {
        group.name = name(&group.name)?;
        if group.id.is_empty()
            || group.id.len() > 64
            || group.id == "unassigned"
            || group.position < 0
            || !names.insert(group.name.to_lowercase())
        {
            return Err(ApiError::bad("备份分组无效或名称重复"));
        }
        let id = uuid::Uuid::new_v4().to_string();
        if ids.insert(group.id.clone(), id.clone()).is_some() {
            return Err(ApiError::bad("备份分组 ID 重复"));
        }
        group.id = id;
        group.position = position as i64;
        group.revision = 1;
    }
    Ok((groups, ids))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn group(id: &str, name: &str, position: i64) -> Group {
        Group {
            id: id.into(),
            name: name.into(),
            position,
            revision: 7,
        }
    }
    #[test]
    fn rejects_ambiguous_or_reserved_group_references() {
        for invalid in [
            vec![group("a", "Work", 0), group("b", "work", 1)],
            vec![group("a", "Ä", 0), group("b", "ä", 1)],
            vec![group("a", "First", 0), group("a", "Second", 1)],
            vec![group("unassigned", "Custom", 0)],
            vec![group("a", "未分组", 0)],
            vec![group("a", "Negative order", -1)],
        ] {
            assert!(prepare(invalid).is_err());
        }
        let (groups, mapping) = prepare(vec![group("b", "Second", 8), group("a", "First", 2)])
            .unwrap_or_else(|error| panic!("{}", error.1));
        assert_eq!(groups[0].name, "First");
        assert_eq!(groups[0].position, 0);
        assert_eq!(groups[0].revision, 1);
        assert_eq!(mapping["a"], groups[0].id);
        assert_ne!(mapping["a"], "a");
    }
}
