use std::error::Error;

use serde::Deserialize;
use serde::Serialize;
use serde_json::Error as SerdeError;
use serde_with::serde_as;

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ConfigProperties {
    pub uri: Option<String>,
    pub exchange_name: Option<String>,
    pub exchange: Option<String>,
    pub currency: Option<String>,
    pub instrument_kind: Option<String>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Config {
    pub name: String,
    pub properties: Option<ConfigProperties>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CapabilityComponent {
    pub name: String,
    #[serde(rename = "type")]
    pub component_type: String,
    pub properties: Option<Properties>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Properties {
    pub image: String,
    pub config: Option<Vec<Config>>,
}

impl AsRef<Properties> for Properties {
    fn as_ref(&self) -> &Properties {
        self
    }
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Manifest {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: Metadata,
    pub spec: Spec,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Metadata {
    pub name: String,
    pub annotations: Annotations,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Annotations {
    pub description: String,
    pub version: String,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Spec {
    pub components: Vec<CapabilityComponent>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Manifests {
    #[serde(rename = "v0.0.1")]
    pub version: Manifest,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Root {
    pub manifests: Manifests,
    pub deployed_version: Option<String>,
}

/// Extracts capability components from the given JSON configuration string.
///
/// This function parses the provided JSON string representing a configuration,
/// filters the components to find those of type `"capability"` that have a
/// `config`, and returns a vector of these capability components.
///
/// # Arguments
///
/// * `config` - A string slice that holds the JSON configuration.
///
/// # Returns
///
/// A vector of `CapabilityComponent` structs that match the criteria of being
/// of type `"capability"` and having a `config`.
///
/// # Panics
///
/// This function will panic if the JSON string cannot be parsed into the
/// expected structure.
pub fn extract_capability_components(config: &str) -> Result<Vec<CapabilityComponent>, SerdeError> {
    let parsed = serde_json::from_str::<Root>(config)?;
    let components = &parsed.manifests.version.spec.components;
    let capability_components: Vec<CapabilityComponent> = components
        .iter()
        .filter(|comp| {
            comp.component_type == "capability"
                && comp
                    .properties
                    .as_ref()
                    .map_or(false, |p| p.config.is_some())
        })
        .cloned()
        .collect();

    Ok(capability_components)
}

pub type GetFn = fn(&str, &str) -> Result<Option<Vec<u8>>, Box<dyn Error>>;

/// Fetches the manifest configuration for a specific application.
///
/// This function retrieves a list of application names from a manifest,
/// searches for a specific application name, and then retrieves the
/// configuration for that application. It uses a generic `get_fn`
/// function to interact with the key-value store.
///
/// # Arguments
///
/// * `get_fn` - A generic function that follows the signature `fn(&str, &str) -> Result<Option<Vec<u8>>, Box<dyn Error>>`.
///              This function is responsible for retrieving data from a key-value store.
/// * `app_name` - The name of the application to retrieve the configuration for.
/// * `wadm_manifest` - The name of the bucket containing the application manifests.
/// * `wadm_default_manifest` - The key in the bucket that holds the list of application names.
///
/// # Errors
///
/// This function will return an error if:
/// - There is a failure in retrieving the list of applications.
/// - The application name is not found in the list.
/// - There is a failure in retrieving the configuration for the found
///   application.
/// - There is an issue converting the configuration bytes into a `String`.
///
/// # Returns
///
/// A `Result` containing:
/// - `Ok(String)` with the configuration as a `String` if successful.
/// - `Err(Box<dyn Error>)` if any step of the process fails.
pub fn get_manifests(
    get_fn: GetFn,
    app_name: &str,
    wadm_manifest: &str,
    wadm_default_manifest: &str,
) -> Result<String, Box<dyn Error>> {
    let apps = match get_fn(wadm_manifest, wadm_default_manifest) {
        Ok(app_name) => match app_name {
            Some(apps) => match serde_json::from_slice::<Vec<String>>(&apps) {
                Ok(apps) => apps,
                Err(e) => Err(format!(
                    "Failed to parse app names from default manifest: {e}"
                ))?,
            },
            None => Err("Failed to get app name from default manifest")?,
        },
        Err(e) => Err(e)?,
    };

    let app = match apps.iter().find(|&app| app == app_name) {
        Some(app) => app,
        None => Err(format!("App {app_name} not found"))?,
    };

    let app_key = format!("{}-{}", wadm_default_manifest, app);
    let config = get_fn(wadm_manifest, &app_key);
    match config {
        Ok(config) => match config {
            Some(config) => Ok(String::from_utf8(config)?),
            None => Err("Failed to get config for app: Config not found")?,
        },
        Err(e) => Err(format!("Failed to get config for app: {e}"))?,
    }
}

#[cfg(test)]
mod tests {
    use std::{fs::File, io::Read as _};

    use super::*;

    // A mock function to simulate the behavior of the key-value store `get` function
    fn mock_get(bucket: &str, key: &str) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        match (bucket, key) {
            // Simulate the retrieval of the list of application names
            ("wadm_manifests", "default") => Ok(Some(Vec::from(r#"["mds", "another-app"]"#))),
            // Simulate the retrieval of the configuration for a specific app
            ("wadm_manifests", "default-mds") => Ok(Some(Vec::from(r#"{"config": "value"}"#))),
            _ => Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Key not found",
            ))),
        }
    }

    #[test]
    fn test_get_manifests() {
        let app_name = "mds";
        let wadm_manifest = "wadm_manifests";
        let wadm_default_manifest = "default";

        // Call `get_manifests` with the mock `get_fn` function
        let result = get_manifests(mock_get, app_name, wadm_manifest, wadm_default_manifest);

        // Assert that the configuration is returned correctly
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), r#"{"config": "value"}"#);
    }

    #[test]
    fn test_get_manifests_app_not_found() {
        let app_name = "non_existent_app";
        let wadm_manifest = "wadm_manifests";
        let wadm_default_manifest = "default";

        // Call `get_manifests` with the mock `get_fn` function
        let result = get_manifests(mock_get, app_name, wadm_manifest, wadm_default_manifest);

        // Assert that an error is returned when the app is not found
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "App non_existent_app not found"
        );
    }

    #[test]
    fn test_extract_capability_components() {
        let json_data = r#"
        {
            "manifests": {
                "v0.0.1": {
                    "apiVersion": "core.oam.dev/v1beta1",
                    "kind": "Application",
                    "metadata": {
                        "name": "mds",
                        "annotations": {
                            "description": "HTTP hello world demo in Rust, using the WebAssembly Component Model and WebAssembly Interfaces Types (WIT)",
                            "version": "v0.0.1"
                        }
                    },
                    "spec": {
                        "components": [
                            {
                                "name": "future-ticker-deribit-btc",
                                "type": "capability",
                                "properties": {
                                    "image": "ghcr.io/jabratech/ticker-provider:0.1.0",
                                    "config": [
                                        {
                                            "name": "future-ticker-deribit-btc",
                                            "properties": {
                                                "exchange_name": "jabratech",
                                                "uri": "192.100.1.213:4222",
                                                "exchange": "deribit",
                                                "currency": "btc",
                                                "instrument_kind": "future"
                                            }
                                        }
                                    ]
                                }
                            },
                            {
                                "name": "option-ticker-deribit-btc",
                                "type": "capability",
                                "properties": {
                                    "image": "ghcr.io/jabratech/ticker-provider:0.1.0",
                                    "config": [
                                        {
                                            "name": "option-ticker-deribit-btc",
                                            "properties": {
                                                "exchange_name": "deribit",
                                                "currency": "btc",
                                                "instrument_kind": "option",
                                                "exchange": "deribit",
                                                "uri": "192.100.1.213:4222"
                                            }
                                        }
                                    ]
                                }
                            },
                            {
                                "name": "another-component",
                                "type": "component",
                                "properties": {
                                    "image": "ghcr.io/jabratech/another-component:0.1.0"
                                }
                            }
                        ]
                    }
                }
            },
            "deployed_version": null
        }
        "#;

        let expected = vec![
            CapabilityComponent {
                name: "future-ticker-deribit-btc".to_string(),
                component_type: "capability".to_string(),
                properties: Some(Properties {
                    image: "ghcr.io/jabratech/ticker-provider:0.1.0".to_string(),
                    config: Some(vec![Config {
                        name: "future-ticker-deribit-btc".to_string(),
                        properties: Some(ConfigProperties {
                            exchange_name: Some("jabratech".to_string()),
                            uri: Some("192.100.1.213:4222".to_string()),
                            exchange: Some("deribit".to_string()),
                            currency: Some("btc".to_string()),
                            instrument_kind: Some("future".to_string()),
                        }),
                    }]),
                }),
            },
            CapabilityComponent {
                name: "option-ticker-deribit-btc".to_string(),
                component_type: "capability".to_string(),
                properties: Some(Properties {
                    image: "ghcr.io/jabratech/ticker-provider:0.1.0".to_string(),
                    config: Some(vec![Config {
                        name: "option-ticker-deribit-btc".to_string(),
                        properties: Some(ConfigProperties {
                            exchange_name: Some("deribit".to_string()),
                            uri: Some("192.100.1.213:4222".to_string()),
                            exchange: Some("deribit".to_string()),
                            currency: Some("btc".to_string()),
                            instrument_kind: Some("option".to_string()),
                        }),
                    }]),
                }),
            },
        ];

        let result = extract_capability_components(json_data).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_extract_capability_components_from_file() -> Result<(), SerdeError> {
        // Specify the path to your JSON file
        let path = "../manifestor/tests/app_manifest.json";

        // Open the file
        let mut file = File::open(path).expect("File not found");

        // Read the contents of the file into a string
        let mut json_data = String::new();
        file.read_to_string(&mut json_data)
            .expect("Failed to read the file");

        // Attempt to deserialize the JSON data into the Root struct
        let result = extract_capability_components(&json_data);

        // Ensure that deserialization succeeds
        assert!(
            result.is_ok(),
            "Failed to deserialize JSON: {:?}",
            result.err()
        );

        // Optionally, check the contents of the deserialized data
        let capability_components = result.unwrap();
        assert!(
            !capability_components.is_empty(),
            "No capability components found"
        );

        Ok(())
    }
}
