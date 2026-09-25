use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
};

use fs::Fs;
use serde::Deserialize;
use serde_json_lenient::Value;
use util::normalize_path;

use crate::{
    devcontainer_api::DevContainerError,
    devcontainer_json::{FeatureOptionValue, FeatureOptions, MountDefinition},
    safe_id_upper,
};

/// Parsed components of an OCI feature reference such as
/// `ghcr.io/devcontainers/features/aws-cli:1`.
///
/// Mirrors the CLI's `OCIRef` in `containerCollectionsOCI.ts`.
#[derive(Debug, Clone)]
pub(crate) struct OciFeatureRef {
    /// Registry hostname, e.g. `ghcr.io`
    pub registry: String,
    /// Full repository path within the registry, e.g. `devcontainers/features/aws-cli`
    pub path: String,
    /// Version tag, digest, or `latest`
    pub version: String,
}

/// Minimal representation of a `devcontainer-feature.json` file, used to
/// extract option default values after the feature tarball is downloaded.
///
/// See: https://containers.dev/implementors/features/#devcontainer-featurejson-properties
#[derive(Debug, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DevContainerFeatureJson {
    pub(crate) id: Option<String>,
    pub(crate) version: Option<String>,
    #[serde(default)]
    pub(crate) options: HashMap<String, FeatureOptionDefinition>,
    pub(crate) mounts: Option<Vec<MountDefinition>>,
    pub(crate) init: Option<bool>,
    pub(crate) privileged: Option<bool>,
    pub(crate) entrypoint: Option<String>,
    pub(crate) container_env: Option<HashMap<String, String>>,
    pub(crate) cap_add: Option<Vec<String>>,
    pub(crate) security_opt: Option<Vec<String>>,
    pub(crate) customizations: Option<Value>,
    pub(crate) on_create_command: Option<Value>,
    pub(crate) update_content_command: Option<Value>,
    pub(crate) post_create_command: Option<Value>,
    pub(crate) post_start_command: Option<Value>,
    pub(crate) post_attach_command: Option<Value>,
    pub(crate) installs_after: Option<Vec<String>>,
    /// Values are kept raw so that a malformed entry is reported against the
    /// dependency declaring it, instead of making the whole manifest unreadable.
    pub(crate) depends_on: Option<HashMap<String, Value>>,
    pub(crate) legacy_ids: Option<Vec<String>>,
}

/// `devcontainer-lock.json`, which pins the features a configuration uses to the
/// exact versions resolved earlier, like the reference CLI.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, Deserialize)]
pub(crate) struct FeatureLockfile {
    #[serde(default)]
    pub(crate) features: BTreeMap<String, LockedFeature>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LockedFeature {
    pub(crate) version: String,
    /// The feature's reference by manifest digest, e.g.
    /// `ghcr.io/devcontainers/features/node@sha256:…`.
    pub(crate) resolved: String,
    /// The digest of the feature's layer.
    pub(crate) integrity: String,
}

impl FeatureLockfile {
    /// The lockfile next to the configuration file `config_file_name`. Like the
    /// reference CLI, `.devcontainer.json` gets a hidden one.
    pub(crate) fn file_name(config_file_name: &str) -> &'static str {
        if config_file_name.starts_with('.') {
            ".devcontainer-lock.json"
        } else {
            "devcontainer-lock.json"
        }
    }

    pub(crate) fn to_json(&self) -> Result<String, serde_json::Error> {
        Ok(format!("{}\n", serde_json::to_string_pretty(self)?))
    }

    /// The manifest reference to fetch `feature_ref` by: its locked digest, or else
    /// the tag it names.
    pub(crate) fn reference<'a>(&'a self, feature_ref: &str, tag: &'a str) -> &'a str {
        self.features
            .get(feature_ref)
            .and_then(|locked| locked.resolved.rsplit_once('@'))
            .map_or(tag, |(_, digest)| digest)
    }
}

/// A single option definition inside `devcontainer-feature.json`.
/// We only need the `default` field to populate env variables.
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub(crate) struct FeatureOptionDefinition {
    pub(crate) default: Option<Value>,
}

impl FeatureOptionDefinition {
    fn serialize_default(&self) -> Option<String> {
        self.default.as_ref().map(|some_value| match some_value {
            Value::Bool(b) => b.to_string(),
            Value::String(s) => s.to_string(),
            Value::Number(n) => n.to_string(),
            other => other.to_string(),
        })
    }
}

#[derive(Debug, Eq, PartialEq, Default)]
pub(crate) struct FeatureManifest {
    consecutive_id: String,
    user_feature_id: String,
    file_path: PathBuf,
    feature_json: DevContainerFeatureJson,
}

/// A Dockerfile `ENV` line, quoted like the reference CLI's `generateContainerEnvs`,
/// so values with spaces stay one value. `$` isn't escaped: values like
/// `${PATH}:/opt/bin` are meant to expand.
pub(crate) fn dockerfile_env(key: &str, value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if character == '"' || character == '\\' {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    format!("ENV {key}=\"{escaped}\"\n")
}

impl FeatureManifest {
    pub(crate) fn new(
        consecutive_id: String,
        user_feature_id: String,
        file_path: PathBuf,
        feature_json: DevContainerFeatureJson,
    ) -> Self {
        Self {
            consecutive_id,
            user_feature_id,
            file_path,
            feature_json,
        }
    }
    pub(crate) fn container_env(&self) -> HashMap<String, String> {
        self.feature_json.container_env.clone().unwrap_or_default()
    }

    pub(crate) fn generate_dockerfile_feature_layer(
        &self,
        use_buildkit: bool,
        dest: &str,
    ) -> String {
        let id = &self.consecutive_id;
        if use_buildkit {
            format!(
                r#"
RUN --mount=type=bind,from=dev_containers_feature_content_source,source=./{id},target=/tmp/build-features-src/{id} \
cp -ar /tmp/build-features-src/{id} {dest} \
&& chmod -R 0755 {dest}/{id} \
&& cd {dest}/{id} \
&& chmod +x ./devcontainer-features-install.sh \
&& ./devcontainer-features-install.sh \
&& rm -rf {dest}/{id}
"#,
            )
        } else {
            let source = format!("/tmp/build-features/{id}");
            let full_dest = format!("{dest}/{id}");
            format!(
                r#"
COPY --chown=root:root --from=dev_containers_feature_content_source {source} {full_dest}
RUN chmod -R 0755 {full_dest} \
&& cd {full_dest} \
&& chmod +x ./devcontainer-features-install.sh \
&& ./devcontainer-features-install.sh
"#
            )
        }
    }

    pub(crate) fn generate_dockerfile_env(&self) -> String {
        let mut layer = "".to_string();
        let env = self.container_env();
        let mut env: Vec<(&String, &String)> = env.iter().collect();
        env.sort();

        for (key, value) in env {
            layer.push_str(&dockerfile_env(key, value));
        }
        layer
    }

    /// Merges user options from devcontainer.json with default options defined in this feature manifest
    pub(crate) fn generate_merged_env(&self, options: &FeatureOptions) -> HashMap<String, String> {
        let mut merged: HashMap<String, String> = self
            .feature_json
            .options
            .iter()
            .filter_map(|(k, v)| {
                v.serialize_default()
                    .map(|v_some| (safe_id_upper(k), v_some))
            })
            .collect();

        match options {
            FeatureOptions::Bool(_) => {}
            FeatureOptions::String(version) => {
                merged.insert("VERSION".to_string(), version.clone());
            }
            FeatureOptions::Options(map) => {
                for (key, value) in map {
                    merged.insert(safe_id_upper(key), value.to_string());
                }
            }
        }
        merged
    }

    pub(crate) async fn write_feature_env(
        &self,
        fs: &Arc<dyn Fs>,
        options: &FeatureOptions,
    ) -> Result<String, DevContainerError> {
        let merged_env = self.generate_merged_env(options);

        let mut env_vars: Vec<(&String, &String)> = merged_env.iter().collect();
        env_vars.sort();

        let env_file_content = env_vars
            .iter()
            .fold("".to_string(), |acc, (k, v)| format!("{acc}{}={}\n", k, v));

        fs.write(
            &self.file_path.join("devcontainer-features.env"),
            env_file_content.as_bytes(),
        )
        .await
        .map_err(|e| {
            log::error!("error writing devcontainer feature environment: {e}");
            DevContainerError::FilesystemError
        })?;

        Ok(env_file_content)
    }

    pub(crate) fn mounts(&self) -> Vec<MountDefinition> {
        if let Some(mounts) = &self.feature_json.mounts {
            mounts.clone()
        } else {
            vec![]
        }
    }

    pub(crate) fn init(&self) -> bool {
        self.feature_json.init.unwrap_or(false)
    }

    pub(crate) fn privileged(&self) -> bool {
        self.feature_json.privileged.unwrap_or(false)
    }

    pub(crate) fn entrypoint(&self) -> Option<String> {
        self.feature_json.entrypoint.clone()
    }

    pub(crate) fn cap_add(&self) -> Vec<String> {
        self.feature_json.cap_add.clone().unwrap_or_default()
    }

    pub(crate) fn security_opt(&self) -> Vec<String> {
        self.feature_json.security_opt.clone().unwrap_or_default()
    }

    pub(crate) fn file_path(&self) -> PathBuf {
        self.file_path.clone()
    }

    pub(crate) fn build_metadata_entry(
        &self,
    ) -> serde_json_lenient::Map<String, serde_json_lenient::Value> {
        use serde_json_lenient::Value;
        let mut entry = serde_json_lenient::Map::new();
        entry.insert(
            "id".to_string(),
            Value::String(self.user_feature_id.clone()),
        );
        if let Some(true) = self.feature_json.init {
            entry.insert("init".to_string(), Value::Bool(true));
        }
        if let Some(true) = self.feature_json.privileged {
            entry.insert("privileged".to_string(), Value::Bool(true));
        }
        if let Some(caps) = &self.feature_json.cap_add {
            if !caps.is_empty() {
                entry.insert(
                    "capAdd".to_string(),
                    Value::Array(caps.iter().map(|s| Value::String(s.clone())).collect()),
                );
            }
        }
        if let Some(opts) = &self.feature_json.security_opt {
            if !opts.is_empty() {
                entry.insert(
                    "securityOpt".to_string(),
                    Value::Array(opts.iter().map(|s| Value::String(s.clone())).collect()),
                );
            }
        }
        if let Some(ep) = &self.feature_json.entrypoint {
            entry.insert("entrypoint".to_string(), Value::String(ep.clone()));
        }
        if let Some(mounts) = &self.feature_json.mounts {
            if !mounts.is_empty() {
                entry.insert(
                    "mounts".to_string(),
                    Value::Array(
                        mounts
                            .iter()
                            .filter_map(|mount| serde_json_lenient::to_value(mount).ok())
                            .collect(),
                    ),
                );
            }
        }
        if let Some(customizations) = &self.feature_json.customizations {
            if !customizations.is_null() {
                entry.insert("customizations".to_string(), customizations.clone());
            }
        }
        for (key, value) in [
            ("onCreateCommand", &self.feature_json.on_create_command),
            (
                "updateContentCommand",
                &self.feature_json.update_content_command,
            ),
            ("postCreateCommand", &self.feature_json.post_create_command),
            ("postStartCommand", &self.feature_json.post_start_command),
            ("postAttachCommand", &self.feature_json.post_attach_command),
        ] {
            if let Some(value) = value {
                if !value.is_null() {
                    entry.insert(key.to_string(), value.clone());
                }
            }
        }
        entry
    }
}

/// Parses an OCI feature reference string into its components.
///
/// Handles formats like:
/// - `ghcr.io/devcontainers/features/aws-cli:1`
/// - `ghcr.io/user/repo/go`  (implicitly `:latest`)
/// - `ghcr.io/devcontainers/features/rust@sha256:abc123`
///
/// Returns `None` for local paths (`./…`) and direct tarball URIs (`https://…`).
pub(crate) fn parse_oci_feature_ref(input: &str) -> Option<OciFeatureRef> {
    if input.starts_with('.')
        || input.starts_with('/')
        || input.starts_with("https://")
        || input.starts_with("http://")
    {
        return None;
    }

    let input_lower = input.to_lowercase();

    let (resource, version) = if let Some(at_idx) = input_lower.rfind('@') {
        // Digest-based: ghcr.io/foo/bar@sha256:abc
        (
            input_lower[..at_idx].to_string(),
            input_lower[at_idx + 1..].to_string(),
        )
    } else {
        let last_slash = input_lower.rfind('/');
        let last_colon = input_lower.rfind(':');
        match (last_slash, last_colon) {
            (Some(slash), Some(colon)) if colon > slash => (
                input_lower[..colon].to_string(),
                input_lower[colon + 1..].to_string(),
            ),
            _ => (input_lower, "latest".to_string()),
        }
    };

    let parts: Vec<&str> = resource.split('/').collect();
    if parts.len() < 3 {
        return None;
    }

    let registry = parts[0].to_string();
    let path = parts[1..].join("/");

    Some(OciFeatureRef {
        registry,
        path,
        version,
    })
}

/// Where a feature comes from, normalized so that references which name the
/// same feature differently (casing, implicit `latest`, relative paths)
/// compare equal.
///
/// Mirrors the CLI's `sourceInformation` as used by `containerFeaturesOrder.ts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FeatureSource {
    Oci {
        registry_and_namespace: String,
        id: String,
        tag: String,
    },
    Local {
        resolved_path: PathBuf,
    },
    Tarball {
        uri: String,
    },
    Other {
        id: String,
    },
}

impl FeatureSource {
    pub(crate) fn from_reference(reference: &str, config_directory: &Path) -> Self {
        if reference.starts_with("./") || reference.starts_with("../") {
            return FeatureSource::Local {
                resolved_path: normalize_path(&config_directory.join(reference)),
            };
        }
        if reference.starts_with("https://") || reference.starts_with("http://") {
            return FeatureSource::Tarball {
                uri: reference.to_string(),
            };
        }
        match parse_oci_feature_ref(reference) {
            Some(oci_reference) => match oci_reference.path.rsplit_once('/') {
                Some((namespace, id)) => FeatureSource::Oci {
                    registry_and_namespace: format!("{}/{}", oci_reference.registry, namespace),
                    id: id.to_string(),
                    tag: oci_reference.version,
                },
                None => FeatureSource::Other {
                    id: reference.to_lowercase(),
                },
            },
            None => FeatureSource::Other {
                id: reference.to_lowercase(),
            },
        }
    }
}

/// A feature taking part in install order resolution: either declared in
/// `devcontainer.json` or pulled in through another feature's `dependsOn`.
#[derive(Debug, Clone)]
pub(crate) struct FeatureOrderNode {
    pub(crate) user_feature_id: String,
    pub(crate) source: FeatureSource,
    pub(crate) options: FeatureOptions,
    /// Hard dependencies: satisfied only by a feature with the same source
    /// and equivalent options.
    pub(crate) depends_on: Vec<(FeatureSource, FeatureOptions)>,
    /// Soft dependencies: satisfied by any version of the named feature, and
    /// ignored when that feature isn't part of the installation.
    pub(crate) installs_after: Vec<FeatureSource>,
    /// The feature's own id followed by its `legacyIds`, so that references
    /// to a renamed feature still match it.
    pub(crate) aliases: Vec<String>,
}

impl FeatureOrderNode {
    pub(crate) fn new(
        user_feature_id: String,
        source: FeatureSource,
        options: FeatureOptions,
    ) -> Self {
        Self {
            user_feature_id,
            source,
            options,
            depends_on: Vec::new(),
            installs_after: Vec::new(),
            aliases: Vec::new(),
        }
    }

    pub(crate) fn is_same_feature(&self, source: &FeatureSource, options: &FeatureOptions) -> bool {
        self.source == *source && compare_feature_options(&self.options, options) == Ordering::Equal
    }

    /// Mirrors the CLI's `satisfiesSoftDependency`, which ignores versions and
    /// options and only asks whether this is the named feature.
    fn satisfies_soft_dependency(&self, dependency: &FeatureSource) -> bool {
        match (&self.source, dependency) {
            (
                FeatureSource::Oci {
                    registry_and_namespace,
                    id,
                    ..
                },
                FeatureSource::Oci {
                    registry_and_namespace: dependency_registry_and_namespace,
                    id: dependency_id,
                    ..
                },
            ) => {
                registry_and_namespace == dependency_registry_and_namespace
                    && (id == dependency_id || self.aliases.contains(dependency_id))
            }
            (
                FeatureSource::Local { resolved_path },
                FeatureSource::Local {
                    resolved_path: dependency_path,
                },
            ) => resolved_path == dependency_path,
            (
                FeatureSource::Tarball { uri },
                FeatureSource::Tarball {
                    uri: dependency_uri,
                },
            ) => uri == dependency_uri,
            (FeatureSource::Other { id }, FeatureSource::Other { id: dependency_id }) => {
                id == dependency_id
            }
            _ => false,
        }
    }

    fn shares_alias_with(&self, other: &FeatureOrderNode) -> bool {
        let own_ids = self.ids_for_alias_matching();
        let other_ids = other.ids_for_alias_matching();
        own_ids.iter().any(|id| other_ids.contains(id))
    }

    fn ids_for_alias_matching(&self) -> Vec<&str> {
        if !self.aliases.is_empty() {
            return self.aliases.iter().map(String::as_str).collect();
        }
        match &self.source {
            FeatureSource::Oci { id, .. } => vec![id.as_str()],
            _ => Vec::new(),
        }
    }
}

/// `true` and `{}` both mean "enabled with defaults", and a bare string is
/// shorthand for `{ "version": ... }`, so these must compare equal when
/// deciding whether a `dependsOn` entry is already satisfied.
fn normalized_feature_options(
    options: &FeatureOptions,
) -> Option<BTreeMap<&str, FeatureOptionValue>> {
    match options {
        FeatureOptions::Bool(false) => None,
        FeatureOptions::Bool(true) => Some(BTreeMap::new()),
        FeatureOptions::String(version) => Some(BTreeMap::from([(
            "version",
            FeatureOptionValue::String(version.clone()),
        )])),
        FeatureOptions::Options(map) => Some(
            map.iter()
                .map(|(key, value)| (key.as_str(), value.clone()))
                .collect(),
        ),
    }
}

fn compare_feature_option_values(
    left: &FeatureOptionValue,
    right: &FeatureOptionValue,
) -> Ordering {
    match (left, right) {
        (FeatureOptionValue::Bool(left), FeatureOptionValue::Bool(right)) => left.cmp(right),
        (FeatureOptionValue::String(left), FeatureOptionValue::String(right)) => left.cmp(right),
        // Same tie-break as the CLI, which compares `typeof` names.
        (FeatureOptionValue::Bool(_), FeatureOptionValue::String(_)) => Ordering::Less,
        (FeatureOptionValue::String(_), FeatureOptionValue::Bool(_)) => Ordering::Greater,
    }
}

/// Mirrors the CLI's `optionsCompareTo`: fewer keys first, then key by key.
fn compare_feature_options(left: &FeatureOptions, right: &FeatureOptions) -> Ordering {
    match (
        normalized_feature_options(left),
        normalized_feature_options(right),
    ) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(left), Some(right)) => left.len().cmp(&right.len()).then_with(|| {
            left.iter()
                .zip(right.iter())
                .map(|((left_key, left_value), (right_key, right_value))| {
                    left_key
                        .cmp(right_key)
                        .then_with(|| compare_feature_option_values(left_value, right_value))
                })
                .find(|ordering| *ordering != Ordering::Equal)
                .unwrap_or(Ordering::Equal)
        }),
    }
}

/// Mirrors the CLI's `compareTo`, used to sort features installed in the same
/// round. The final comparison on the user-facing id keeps the result
/// deterministic where the CLI would fall back to manifest digests.
fn compare_features_within_round(left: &FeatureOrderNode, right: &FeatureOrderNode) -> Ordering {
    let ordering = match (&left.source, &right.source) {
        (
            FeatureSource::Oci {
                registry_and_namespace: left_namespace,
                id: left_id,
                tag: left_tag,
            },
            FeatureSource::Oci {
                registry_and_namespace: right_namespace,
                id: right_id,
                tag: right_tag,
            },
        ) => left_namespace
            .cmp(right_namespace)
            .then_with(|| {
                if left_id == right_id || left.shares_alias_with(right) {
                    Ordering::Equal
                } else {
                    left_id.cmp(right_id)
                }
            })
            .then_with(|| left_tag.cmp(right_tag))
            .then_with(|| compare_feature_options(&left.options, &right.options)),
        (
            FeatureSource::Local {
                resolved_path: left_path,
            },
            FeatureSource::Local {
                resolved_path: right_path,
            },
        ) => left_path
            .cmp(right_path)
            .then_with(|| compare_feature_options(&left.options, &right.options)),
        (FeatureSource::Tarball { uri: left_uri }, FeatureSource::Tarball { uri: right_uri }) => {
            left_uri
                .cmp(right_uri)
                .then_with(|| compare_feature_options(&left.options, &right.options))
        }
        (FeatureSource::Other { id: left_id }, FeatureSource::Other { id: right_id }) => left_id
            .cmp(right_id)
            .then_with(|| compare_feature_options(&left.options, &right.options)),
        _ => left.user_feature_id.cmp(&right.user_feature_id),
    };
    ordering.then_with(|| left.user_feature_id.cmp(&right.user_feature_id))
}

/// Computes the feature installation order, returning indices into `nodes`.
///
/// Mirrors the CLI's `computeDependsOnInstallationOrder` in
/// `containerFeaturesOrder.ts`: features are installed in rounds, each round
/// taking every feature whose `dependsOn` and `installsAfter` are already
/// installed. `overrideFeatureInstallOrder` only raises a feature's priority
/// within those rounds, so it can never break a `dependsOn` requirement.
///
/// `nodes` must already contain every `dependsOn` target, deduplicated with
/// [`FeatureOrderNode::is_same_feature`]; a requirement that can never be met
/// is reported like a cycle.
pub(crate) fn compute_feature_install_order(
    nodes: &[FeatureOrderNode],
    override_install_order: &[FeatureSource],
) -> Result<Vec<usize>, DevContainerError> {
    let override_count = override_install_order.len();
    let round_priorities: Vec<usize> = nodes
        .iter()
        .map(|node| {
            override_install_order
                .iter()
                .enumerate()
                .filter(|(_, source)| node.satisfies_soft_dependency(source))
                .map(|(position, _)| override_count - position)
                .max()
                .unwrap_or(0)
        })
        .collect();

    // Soft dependencies on features that aren't being installed would
    // otherwise block their dependents forever.
    let relevant_installs_after: Vec<Vec<&FeatureSource>> = nodes
        .iter()
        .map(|node| {
            node.installs_after
                .iter()
                .filter(|dependency| {
                    nodes
                        .iter()
                        .any(|candidate| candidate.satisfies_soft_dependency(dependency))
                })
                .collect()
        })
        .collect();

    let mut remaining: Vec<usize> = (0..nodes.len()).collect();
    let mut installation_order: Vec<usize> = Vec::with_capacity(nodes.len());
    while !remaining.is_empty() {
        let mut round: Vec<usize> = remaining
            .iter()
            .copied()
            .filter(|&index| {
                let hard_dependencies_met =
                    nodes[index].depends_on.iter().all(|(source, options)| {
                        installation_order
                            .iter()
                            .any(|&installed| nodes[installed].is_same_feature(source, options))
                    });
                let soft_dependencies_met =
                    relevant_installs_after[index].iter().all(|dependency| {
                        installation_order.iter().any(|&installed| {
                            nodes[installed].satisfies_soft_dependency(dependency)
                        })
                    });
                hard_dependencies_met && soft_dependencies_met
            })
            .collect();

        if round.is_empty() {
            let mut blocked: Vec<&str> = remaining
                .iter()
                .map(|&index| nodes[index].user_feature_id.as_str())
                .collect();
            blocked.sort_unstable();
            return Err(DevContainerError::FeatureDependencyResolutionFailed(
                format!(
                    "Circular dependency detected between features: {}",
                    blocked.join(", ")
                ),
            ));
        }

        let highest_priority = round
            .iter()
            .map(|&index| round_priorities[index])
            .max()
            .unwrap_or(0);
        round.retain(|&index| round_priorities[index] == highest_priority);
        remaining.retain(|index| !round.contains(index));
        round.sort_by(|&left, &right| compare_features_within_round(&nodes[left], &nodes[right]));
        installation_order.extend(round);
    }

    Ok(installation_order)
}

#[cfg(test)]
mod tests {
    #[test]
    fn dockerfile_env_values_stay_one_value() {
        assert_eq!(
            super::dockerfile_env("NODE_OPTIONS", "--use-system-ca --use-env-proxy"),
            "ENV NODE_OPTIONS=\"--use-system-ca --use-env-proxy\"\n"
        );
        assert_eq!(
            super::dockerfile_env("PATH", "/usr/local/share/nvm/current/bin:${PATH}"),
            "ENV PATH=\"/usr/local/share/nvm/current/bin:${PATH}\"\n"
        );
        assert_eq!(
            super::dockerfile_env("QUOTED", r#"say "hi" \o/"#),
            r#"ENV QUOTED="say \"hi\" \\o/"
"#
        );
    }

    #[test]
    fn lockfile_pins_features_to_their_resolved_digest() {
        let lockfile = super::FeatureLockfile {
            features: std::collections::BTreeMap::from([(
                "ghcr.io/devcontainers/features/node:1".to_string(),
                super::LockedFeature {
                    version: "1.6.3".to_string(),
                    resolved: "ghcr.io/devcontainers/features/node@sha256:abc".to_string(),
                    integrity: "sha256:def".to_string(),
                },
            )]),
        };
        assert_eq!(
            lockfile.reference("ghcr.io/devcontainers/features/node:1", "1"),
            "sha256:abc"
        );
        assert_eq!(
            lockfile.reference("ghcr.io/devcontainers/features/go:1", "1"),
            "1"
        );
        assert_eq!(
            lockfile.to_json().unwrap(),
            "{\n  \"features\": {\n    \"ghcr.io/devcontainers/features/node:1\": {\n      \"version\": \"1.6.3\",\n      \"resolved\": \"ghcr.io/devcontainers/features/node@sha256:abc\",\n      \"integrity\": \"sha256:def\"\n    }\n  }\n}\n"
        );
        assert_eq!(
            super::FeatureLockfile::file_name(".devcontainer.json"),
            ".devcontainer-lock.json"
        );
        assert_eq!(
            super::FeatureLockfile::file_name("devcontainer.json"),
            "devcontainer-lock.json"
        );
    }

    use super::*;

    const CONFIG_DIRECTORY: &str = "/project/.devcontainer";

    fn source(reference: &str) -> FeatureSource {
        FeatureSource::from_reference(reference, Path::new(CONFIG_DIRECTORY))
    }

    fn node(reference: &str) -> FeatureOrderNode {
        FeatureOrderNode::new(
            reference.to_string(),
            source(reference),
            FeatureOptions::Options(HashMap::new()),
        )
    }

    fn installs_after(mut node: FeatureOrderNode, references: &[&str]) -> FeatureOrderNode {
        node.installs_after = references
            .iter()
            .map(|reference| source(reference))
            .collect();
        node
    }

    fn depends_on(mut node: FeatureOrderNode, references: &[&str]) -> FeatureOrderNode {
        node.depends_on = references
            .iter()
            .map(|reference| (source(reference), FeatureOptions::Options(HashMap::new())))
            .collect();
        node
    }

    fn ordered_ids(nodes: &[FeatureOrderNode], overrides: &[&str]) -> Vec<String> {
        let overrides: Vec<FeatureSource> = overrides.iter().map(|id| source(id)).collect();
        compute_feature_install_order(nodes, &overrides)
            .expect("order should resolve")
            .into_iter()
            .map(|index| nodes[index].user_feature_id.clone())
            .collect()
    }

    #[test]
    fn test_installs_after_chain_is_respected() {
        let nodes = vec![
            installs_after(
                node("ghcr.io/test/features/a:1"),
                &["ghcr.io/test/features/b"],
            ),
            installs_after(
                node("ghcr.io/test/features/b:1"),
                &["ghcr.io/test/features/c"],
            ),
            node("ghcr.io/test/features/c:1"),
        ];
        assert_eq!(
            ordered_ids(&nodes, &[]),
            vec![
                "ghcr.io/test/features/c:1",
                "ghcr.io/test/features/b:1",
                "ghcr.io/test/features/a:1",
            ]
        );
    }

    #[test]
    fn test_installs_after_on_absent_feature_is_ignored() {
        let nodes = vec![
            installs_after(
                node("ghcr.io/test/features/a:1"),
                &["ghcr.io/test/features/not-installed"],
            ),
            node("ghcr.io/test/features/b:1"),
        ];
        assert_eq!(
            ordered_ids(&nodes, &[]),
            vec!["ghcr.io/test/features/a:1", "ghcr.io/test/features/b:1"]
        );
    }

    #[test]
    fn test_installs_after_matches_legacy_ids() {
        let mut renamed = node("ghcr.io/test/features/new-name:1");
        renamed.aliases = vec!["new-name".to_string(), "old-name".to_string()];
        let nodes = vec![
            installs_after(
                node("ghcr.io/test/features/a:1"),
                &["ghcr.io/test/features/old-name"],
            ),
            renamed,
        ];
        assert_eq!(
            ordered_ids(&nodes, &[]),
            vec![
                "ghcr.io/test/features/new-name:1",
                "ghcr.io/test/features/a:1",
            ]
        );
    }

    #[test]
    fn test_depends_on_installs_dependency_first() {
        let nodes = vec![
            depends_on(
                node("ghcr.io/test/features/a:1"),
                &["ghcr.io/test/features/z:2"],
            ),
            node("ghcr.io/test/features/z:2"),
        ];
        assert_eq!(
            ordered_ids(&nodes, &[]),
            vec!["ghcr.io/test/features/z:2", "ghcr.io/test/features/a:1"]
        );
    }

    #[test]
    fn test_depends_on_requires_same_version_and_options() {
        let mut customized = node("ghcr.io/test/features/z:2");
        customized.options = FeatureOptions::Options(HashMap::from([(
            "flavor".to_string(),
            FeatureOptionValue::String("custom".to_string()),
        )]));
        let other_version = node("ghcr.io/test/features/z:3");
        let nodes = vec![
            depends_on(
                node("ghcr.io/test/features/a:1"),
                &["ghcr.io/test/features/z:2"],
            ),
            customized,
            other_version,
        ];
        let error = compute_feature_install_order(&nodes, &[])
            .expect_err("no feature satisfies the dependency exactly");
        assert!(matches!(
            error,
            DevContainerError::FeatureDependencyResolutionFailed(_)
        ));
    }

    #[test]
    fn test_options_shorthands_are_equivalent() {
        let with_defaults = node("ghcr.io/test/features/z:2");
        assert!(with_defaults.is_same_feature(
            &source("ghcr.io/test/features/z:2"),
            &FeatureOptions::Bool(true)
        ));
        let mut with_version = node("ghcr.io/test/features/z:2");
        with_version.options = FeatureOptions::String("1.2".to_string());
        assert!(with_version.is_same_feature(
            &source("GHCR.io/test/features/z:2"),
            &FeatureOptions::Options(HashMap::from([(
                "version".to_string(),
                FeatureOptionValue::String("1.2".to_string()),
            )]))
        ));
        assert!(!with_defaults.is_same_feature(
            &source("ghcr.io/test/features/z:2"),
            &FeatureOptions::Bool(false)
        ));
    }

    #[test]
    fn test_override_takes_priority_over_alphabetical_order() {
        let nodes = vec![
            node("ghcr.io/test/features/a:1"),
            node("ghcr.io/test/features/b:1"),
            node("ghcr.io/test/features/c:1"),
        ];
        assert_eq!(
            ordered_ids(
                &nodes,
                &["ghcr.io/test/features/c", "ghcr.io/test/features/b"]
            ),
            vec![
                "ghcr.io/test/features/c:1",
                "ghcr.io/test/features/b:1",
                "ghcr.io/test/features/a:1",
            ]
        );
    }

    #[test]
    fn test_override_orders_only_features_whose_dependencies_are_installed() {
        let nodes = vec![
            installs_after(
                node("ghcr.io/test/features/a:1"),
                &["ghcr.io/test/features/b"],
            ),
            node("ghcr.io/test/features/b:1"),
            depends_on(
                node("ghcr.io/test/features/c:1"),
                &["ghcr.io/test/features/d:1"],
            ),
            node("ghcr.io/test/features/d:1"),
        ];
        // Neither `a` nor `c` may jump ahead of what it depends on, but once
        // both are eligible the override decides between them.
        assert_eq!(
            ordered_ids(
                &nodes,
                &["ghcr.io/test/features/c", "ghcr.io/test/features/a"]
            ),
            vec![
                "ghcr.io/test/features/b:1",
                "ghcr.io/test/features/d:1",
                "ghcr.io/test/features/c:1",
                "ghcr.io/test/features/a:1",
            ]
        );
    }

    #[test]
    fn test_local_features_are_matched_by_resolved_path() {
        let nodes = vec![
            installs_after(node("./first"), &["../.devcontainer/second"]),
            node("./second"),
        ];
        assert_eq!(
            ordered_ids(&nodes, &["./first"]),
            vec!["./second", "./first"]
        );
    }

    #[test]
    fn test_cycle_is_reported() {
        let nodes = vec![
            depends_on(
                node("ghcr.io/test/features/a:1"),
                &["ghcr.io/test/features/b:1"],
            ),
            installs_after(
                node("ghcr.io/test/features/b:1"),
                &["ghcr.io/test/features/a"],
            ),
            node("ghcr.io/test/features/c:1"),
        ];
        let error = compute_feature_install_order(&nodes, &[]).expect_err("cycle must fail");
        let DevContainerError::FeatureDependencyResolutionFailed(message) = error else {
            panic!("unexpected error: {error:?}");
        };
        assert_eq!(
            message,
            "Circular dependency detected between features: \
             ghcr.io/test/features/a:1, ghcr.io/test/features/b:1"
        );
    }

    #[test]
    fn test_order_is_independent_of_input_order() {
        let nodes = vec![
            node("ghcr.io/test/features/d:1"),
            installs_after(
                node("ghcr.io/test/features/b:1"),
                &["ghcr.io/test/features/c"],
            ),
            node("./local"),
            node("ghcr.io/other/features/a:1"),
            node("ghcr.io/test/features/c:1"),
            node("ghcr.io/test/features/c:2"),
        ];
        let expected = ordered_ids(&nodes, &[]);
        assert_eq!(
            expected,
            vec![
                "./local",
                "ghcr.io/other/features/a:1",
                "ghcr.io/test/features/c:1",
                "ghcr.io/test/features/c:2",
                "ghcr.io/test/features/d:1",
                "ghcr.io/test/features/b:1",
            ]
        );
        let mut reversed = nodes;
        reversed.reverse();
        assert_eq!(ordered_ids(&reversed, &[]), expected);
    }
}
