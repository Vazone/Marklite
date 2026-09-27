use serde::{Deserialize, Serialize};

/// Location only: access grants and file identity are checked by the platform adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ResourceRef {
    DesktopFile { path: String },
    DesktopDirectory { path: String },
    AndroidDocument { uri: String },
    AndroidTree { uri: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_contract_matches_shared_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../src/shared/resource-contract-fixtures.json"
        ))
        .unwrap();
        for value in fixture["valid"].as_array().unwrap() {
            let resource: ResourceRef = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(resource).unwrap(), *value);
        }
        for value in fixture["invalid"].as_array().unwrap() {
            assert!(serde_json::from_value::<ResourceRef>(value.clone()).is_err());
        }
        let session: crate::models::session::ResourceSession =
            serde_json::from_value(fixture["sessionV2"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(&session).unwrap(),
            fixture["sessionV2"]
        );
        let legacy = session.desktop().unwrap();
        assert_eq!(
            serde_json::to_value(crate::models::session::ResourceSession::from(&legacy)).unwrap(),
            fixture["sessionV2"]
        );
    }
}
