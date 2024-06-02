use serde::Deserialize;

fn empty_is_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let s: serde_yaml::Value = serde::de::Deserialize::deserialize(deserializer)?;
    match s {
        serde_yaml::Value::Null => Ok(None),
        serde_yaml::Value::String(s) => {
            if s.is_empty() {
                Ok(None)
            } else {
                Ok(Some(s))
            }
        }
        v => panic!("unexpected value {v:?}"),
    }
}

#[derive(Deserialize, PartialEq, Eq, Debug)]
struct UserAgent {
    family: String,
    #[serde(deserialize_with = "empty_is_none")]
    major: Option<String>,
    #[serde(deserialize_with = "empty_is_none")]
    minor: Option<String>,
    #[serde(deserialize_with = "empty_is_none")]
    patch: Option<String>,
    #[serde(default, deserialize_with = "empty_is_none")]
    patch_minor: Option<String>,
}
impl From<ua_parser::user_agent::ValueRef<'_>> for UserAgent {
    fn from(value: ua_parser::user_agent::ValueRef<'_>) -> Self {
        let value = value.into_owned();
        Self {
            family: value.family,
            major: value.major,
            minor: value.minor,
            patch: value.patch,
            patch_minor: value.patch_minor,
        }
    }
}

#[derive(Deserialize, PartialEq, Eq, Debug)]
pub struct OS {
    pub family: String,
    pub major: Option<String>,
    pub minor: Option<String>,
    pub patch: Option<String>,
    pub patch_minor: Option<String>,
}
impl From<ua_parser::os::ValueRef<'_>> for OS {
    fn from(value: ua_parser::os::ValueRef<'_>) -> Self {
        let value = value.into_owned();
        Self {
            family: value.os,
            major: value.major,
            minor: value.minor,
            patch: value.patch,
            patch_minor: value.patch_minor,
        }
    }
}

#[derive(Deserialize, PartialEq, Eq, Debug)]
pub struct Device {
    pub family: String,
    pub brand: Option<String>,
    pub model: Option<String>,
}
impl From<ua_parser::device::ValueRef<'_>> for Device {
    fn from(value: ua_parser::device::ValueRef<'_>) -> Self {
        let value = value.into_owned();
        Self {
            family: value.device,
            brand: value.brand,
            model: value.model,
        }
    }
}

fn get_extractor() -> Result<
    &'static ua_parser::Extractor<'static>,
    &'static (dyn std::error::Error + Send + Sync + 'static),
> {
    static EXTRACTOR: std::sync::OnceLock<
        Result<ua_parser::Extractor<'static>, Box<dyn std::error::Error + Send + Sync>>,
    > = std::sync::OnceLock::new();

    EXTRACTOR
        .get_or_init(|| {
            let p: std::path::PathBuf = [env!("CARGO_MANIFEST_DIR"), "uap-core", "regexes.yaml"]
                .iter()
                .collect();
            let rs = serde_yaml::from_reader::<_, ua_parser::Regexes>(std::fs::File::open(p)?)?
                .try_into()?;
            Ok(rs)
        })
        .as_ref()
        .map_err(|e| &**e)
}

#[derive(Deserialize)]
struct UaTestCases {
    test_cases: Vec<UaTestCase>,
}
#[derive(Deserialize)]
struct UaTestCase {
    user_agent_string: String,
    #[serde(flatten)]
    ua: UserAgent,
}

#[test]
fn test_ua() {
    let rs = &get_extractor().unwrap().ua;

    let p = [
        env!("CARGO_MANIFEST_DIR"),
        "uap-core",
        "tests",
        "test_ua.yaml",
    ]
    .iter()
    .collect::<std::path::PathBuf>();
    let items = serde_yaml::from_reader::<_, UaTestCases>(std::fs::File::open(p).unwrap()).unwrap();
    for UaTestCase {
        user_agent_string,
        ua,
    } in items.test_cases
    {
        let ua_ = rs.extract(&user_agent_string).map_or_else(
            || UserAgent {
                family: "Other".to_string(),
                major: None,
                minor: None,
                patch: None,
                patch_minor: None,
            },
            From::from,
        );
        assert_eq!(ua, ua_, "{user_agent_string}");
    }
}

#[test]
fn test_ff() {
    let rs = &get_extractor().unwrap().ua;

    let p = [
        env!("CARGO_MANIFEST_DIR"),
        "uap-core",
        "test_resources",
        "firefox_user_agent_strings.yaml",
    ]
    .iter()
    .collect::<std::path::PathBuf>();
    let items = serde_yaml::from_reader::<_, UaTestCases>(std::fs::File::open(p).unwrap()).unwrap();
    for UaTestCase {
        user_agent_string,
        ua,
    } in items.test_cases
    {
        let ua_ = rs.extract(&user_agent_string).map_or_else(
            || UserAgent {
                family: "Other".to_string(),
                major: None,
                minor: None,
                patch: None,
                patch_minor: None,
            },
            From::from,
        );
        assert_eq!(ua, ua_, "{user_agent_string}");
    }
}

#[test]
fn test_pgts() {
    let rs = &get_extractor().unwrap().ua;

    let p = [
        env!("CARGO_MANIFEST_DIR"),
        "uap-core",
        "test_resources",
        "pgts_browser_list.yaml",
    ]
    .iter()
    .collect::<std::path::PathBuf>();
    let items = serde_yaml::from_reader::<_, UaTestCases>(std::fs::File::open(p).unwrap()).unwrap();
    for UaTestCase {
        user_agent_string,
        ua,
    } in items.test_cases
    {
        let ua_ = rs.extract(&user_agent_string).map_or_else(
            || UserAgent {
                family: "Other".to_string(),
                major: None,
                minor: None,
                patch: None,
                patch_minor: None,
            },
            From::from,
        );
        assert_eq!(ua, ua_, "{user_agent_string}");
    }
}

#[test]
fn test_opera() {
    let rs = &get_extractor().unwrap().ua;

    let p = [
        env!("CARGO_MANIFEST_DIR"),
        "uap-core",
        "test_resources",
        "opera_mini_user_agent_strings.yaml",
    ]
    .iter()
    .collect::<std::path::PathBuf>();
    let items = serde_yaml::from_reader::<_, UaTestCases>(std::fs::File::open(p).unwrap()).unwrap();
    for UaTestCase {
        user_agent_string,
        ua,
    } in items.test_cases
    {
        let ua_ = rs.extract(&user_agent_string).map_or_else(
            || UserAgent {
                family: "Other".to_string(),
                major: None,
                minor: None,
                patch: None,
                patch_minor: None,
            },
            From::from,
        );
        assert_eq!(ua, ua_, "{user_agent_string}");
    }
}

#[test]
fn test_podcasting() {
    let rs = &get_extractor().unwrap().ua;

    let p = [
        env!("CARGO_MANIFEST_DIR"),
        "uap-core",
        "test_resources",
        "podcasting_user_agent_strings.yaml",
    ]
    .iter()
    .collect::<std::path::PathBuf>();
    let items = serde_yaml::from_reader::<_, UaTestCases>(std::fs::File::open(p).unwrap()).unwrap();
    for UaTestCase {
        user_agent_string,
        ua,
    } in items.test_cases
    {
        let ua_ = rs.extract(&user_agent_string).map_or_else(
            || UserAgent {
                family: "Other".to_string(),
                major: None,
                minor: None,
                patch: None,
                patch_minor: None,
            },
            From::from,
        );
        assert_eq!(ua, ua_, "{user_agent_string}");
    }
}

#[derive(Deserialize)]
struct DevTestCases {
    test_cases: Vec<DevTestCase>,
}
#[derive(Deserialize)]
struct DevTestCase {
    user_agent_string: String,
    #[serde(flatten)]
    dev: Device,
}

#[test]
fn test_device() {
    let rs = &get_extractor().unwrap().dev;

    let p = [
        env!("CARGO_MANIFEST_DIR"),
        "uap-core",
        "tests",
        "test_device.yaml",
    ]
    .iter()
    .collect::<std::path::PathBuf>();
    let items =
        serde_yaml::from_reader::<_, DevTestCases>(std::fs::File::open(p).unwrap()).unwrap();
    for DevTestCase {
        user_agent_string,
        dev,
    } in items.test_cases
    {
        let dev_ = rs.extract(&user_agent_string).map_or_else(
            || Device {
                family: "Other".to_string(),
                brand: None,
                model: None,
            },
            From::from,
        );
        assert_eq!(dev, dev_, "{user_agent_string}");
    }
}

#[derive(Deserialize)]
struct OSTestCases {
    test_cases: Vec<OSTestCase>,
}
#[derive(Deserialize)]
struct OSTestCase {
    user_agent_string: String,
    #[serde(flatten)]
    os: OS,
}

#[test]
fn test_os() {
    let rs = &get_extractor().unwrap().os;

    let p = [
        env!("CARGO_MANIFEST_DIR"),
        "uap-core",
        "tests",
        "test_os.yaml",
    ]
    .iter()
    .collect::<std::path::PathBuf>();
    let items = serde_yaml::from_reader::<_, OSTestCases>(std::fs::File::open(p).unwrap()).unwrap();
    for OSTestCase {
        user_agent_string,
        os,
    } in items.test_cases
    {
        let os_ = rs.extract(&user_agent_string).map_or_else(
            || OS {
                family: "Other".to_string(),
                major: None,
                minor: None,
                patch: None,
                patch_minor: None,
            },
            From::from,
        );
        assert_eq!(os, os_, "{user_agent_string}");
    }
}

#[test]
fn test_additional_os() {
    let rs = &get_extractor().unwrap().os;

    let p = [
        env!("CARGO_MANIFEST_DIR"),
        "uap-core",
        "test_resources",
        "additional_os_tests.yaml",
    ]
    .iter()
    .collect::<std::path::PathBuf>();
    let items = serde_yaml::from_reader::<_, OSTestCases>(std::fs::File::open(p).unwrap()).unwrap();
    for OSTestCase {
        user_agent_string,
        os,
    } in items.test_cases
    {
        let os_ = rs.extract(&user_agent_string).map_or_else(
            || OS {
                family: "Other".to_string(),
                major: None,
                minor: None,
                patch: None,
                patch_minor: None,
            },
            From::from,
        );
        assert_eq!(os, os_, "{user_agent_string}");
    }
}
