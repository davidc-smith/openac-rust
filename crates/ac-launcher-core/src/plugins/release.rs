//! Plugin releases on GitHub: fetching small release documents (catching
//! the tag a `latest` redirect resolves to, which the curated list uses),
//! reading the releases Atom feed for this client's `wasm-v` releases, and
//! resolving a repository to the release to offer.

use std::sync::Arc;

use super::manifest::{InstallRules, PluginManifest, WASM_TAG_PREFIX};
use super::record_store::PluginReleaseChannel;
use super::rules::github;
use crate::cancel::CancelToken;
use crate::net::{self, HttpTransport};
use crate::updates::{LauncherVersion, UpdateError};
use crate::url::WebUrl;

/// Largest small document (list, `plugin.json`, `.sha256`).
pub const MAXIMUM_DOCUMENT_BYTES: u64 = 256 * 1024;

/// Largest releases feed.
pub const FEED_MAXIMUM_BYTES: u64 = 1024 * 1024;

/// A fetched document and, when a `latest/download` redirect named one, the
/// release tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginReleaseDocument {
    /// The bytes.
    pub content: Vec<u8>,
    /// The tag.
    pub tag: Option<String>,
}

/// How a fetch went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginReleaseFetchResult {
    /// Fetched.
    Success(PluginReleaseDocument),
    /// GitHub is rate limiting (HTTP 429).
    RateLimited,
    /// Unreachable, not found, or redirected to another repository.
    Unavailable,
}

/// Fetches plugin release documents through the host's transport.
#[derive(Clone)]
pub struct PluginReleaseClient {
    transport: Arc<dyn HttpTransport>,
}

impl std::fmt::Debug for PluginReleaseClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PluginReleaseClient")
    }
}

/// The tag in a redirect shaped `/<owner>/<repo>/releases/download/<tag>/...`:
/// `Ok(Some)` for the asked-for repository, `Ok(None)` for another shape,
/// `Err` for a release of another repository.
fn match_tag(target: &WebUrl, owner: &str, repo: &str) -> Result<Option<String>, ()> {
    let segments = target.segments();
    if segments.len() < 5 || segments[2] != "releases" || segments[3] != "download" {
        return Ok(None);
    }
    if !segments[0].eq_ignore_ascii_case(owner) || !segments[1].eq_ignore_ascii_case(repo) {
        return Err(());
    }
    Ok(Some(segments[4].to_owned()))
}

impl PluginReleaseClient {
    /// A client over `transport`.
    pub fn new(transport: Arc<dyn HttpTransport>) -> Self {
        Self { transport }
    }

    /// Fetches a document of at most `maximum_bytes`. Transport failures,
    /// non-200 answers and redirects to another repository are
    /// `Unavailable`; 429 is `RateLimited`; a non-HTTPS hop or an oversized
    /// body is an error.
    pub fn fetch_document(
        &self,
        url: &WebUrl,
        maximum_bytes: u64,
        cancel: &CancelToken,
    ) -> Result<PluginReleaseFetchResult, UpdateError> {
        net::require_secure_or_loopback(url, "plugin release")?;
        let segments = url.segments();
        let (owner, repo) = if segments.len() >= 2 {
            (segments[0].to_owned(), segments[1].to_owned())
        } else {
            (String::new(), String::new())
        };
        let mut tag: Option<String> = None;
        let mut redirected_elsewhere = false;
        let response = net::send_with_validated_redirects(
            self.transport.as_ref(),
            url,
            "plugin release",
            None,
            |next| match match_tag(next, &owner, &repo) {
                Ok(Some(matched)) => {
                    tag.get_or_insert(matched);
                }
                Ok(None) => {}
                Err(()) => redirected_elsewhere = true,
            },
            cancel,
        );
        let mut response = match response {
            Ok(response) => response,
            Err(UpdateError::Transport(_)) => return Ok(PluginReleaseFetchResult::Unavailable),
            Err(error) => return Err(error),
        };
        if redirected_elsewhere {
            return Ok(PluginReleaseFetchResult::Unavailable);
        }
        if response.status == 429 {
            return Ok(PluginReleaseFetchResult::RateLimited);
        }
        if response.status != 200 {
            return Ok(PluginReleaseFetchResult::Unavailable);
        }
        let too_large =
            format!("The plugin release document is larger than {maximum_bytes} bytes.");
        match net::read_bounded(&mut response, maximum_bytes, &too_large, cancel) {
            Ok(content) => Ok(PluginReleaseFetchResult::Success(PluginReleaseDocument {
                content,
                tag,
            })),
            Err(UpdateError::Transport(_)) => Ok(PluginReleaseFetchResult::Unavailable),
            Err(error) => Err(error),
        }
    }
}

// --- The releases feed ----------------------------------------------------------

const ATOM: &str = "http://www.w3.org/2005/Atom";

/// A WebAssembly release found on a feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginFeedRelease {
    /// Its tag (`wasm-v` + version).
    pub tag: String,
    /// Its version.
    pub version: LauncherVersion,
}

/// The feed could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct PluginReleaseFeedError(pub String);

fn match_release(href: &str, repo: &str) -> Option<PluginFeedRelease> {
    let url = WebUrl::parse(href)?;
    if !url.is_https()
        || !url.host().eq_ignore_ascii_case("github.com")
        || url.port() != Some(443)
        || !url.query().is_empty()
        || !url.fragment().is_empty()
    {
        return None;
    }
    let segments = url.segments();
    let (owner, name) = repo.split_once('/')?;
    if segments.len() != 5
        || !segments[0].eq_ignore_ascii_case(owner)
        || !segments[1].eq_ignore_ascii_case(name)
        || segments[2] != "releases"
        || segments[3] != "tag"
    {
        return None;
    }
    let tag = segments[4];
    let version_text = tag.strip_prefix(WASM_TAG_PREFIX)?;
    if version_text.contains('+') {
        return None;
    }
    let version = LauncherVersion::try_parse(version_text)?;
    Some(PluginFeedRelease {
        tag: tag.to_owned(),
        version,
    })
}

/// Every WebAssembly release on a repository's releases feed. Only a `link`
/// that is a direct child of an `entry` counts, naming
/// `https://github.com/<owner>/<repo>/releases/tag/wasm-v<version>` for this
/// repository with no query, fragment or build metadata; any other tag (an
/// OpenAC C# release's `v<version>` among them) is not this client's.
/// Malformed XML or a DTD is an error.
pub fn wasm_releases(
    content: &[u8],
    repo: &str,
) -> Result<Vec<PluginFeedRelease>, PluginReleaseFeedError> {
    let text = std::str::from_utf8(content)
        .map_err(|_| PluginReleaseFeedError("the releases feed is not UTF-8".into()))?;
    // roxmltree refuses a DTD unless asked to allow one.
    let document = roxmltree::Document::parse(text)
        .map_err(|_| PluginReleaseFeedError("the releases feed is not well-formed XML".into()))?;
    let feed = document.root_element();
    if feed.tag_name().name() != "feed" || feed.tag_name().namespace() != Some(ATOM) {
        return Err(PluginReleaseFeedError(
            "the releases feed has no <feed> root".into(),
        ));
    }
    let mut releases = Vec::new();
    for entry in feed
        .children()
        .filter(|node| node.is_element() && node.has_tag_name((ATOM, "entry")))
    {
        for link in entry
            .children()
            .filter(|node| node.is_element() && node.has_tag_name((ATOM, "link")))
        {
            if let Some(release) = link
                .attribute("href")
                .and_then(|href| match_release(href, repo))
            {
                releases.push(release);
            }
        }
    }
    Ok(releases)
}

fn highest(releases: &[PluginFeedRelease], pre_release: bool) -> Option<&PluginFeedRelease> {
    releases
        .iter()
        .filter(|release| release.version.is_pre_release() == pre_release)
        .fold(
            None,
            |best: Option<&PluginFeedRelease>, candidate| match best {
                Some(current) if current.version >= candidate.version => Some(current),
                _ => Some(candidate),
            },
        )
}

/// The highest-precedence WebAssembly pre-release on a releases feed.
pub fn highest_prerelease(
    content: &[u8],
    repo: &str,
) -> Result<Option<PluginFeedRelease>, PluginReleaseFeedError> {
    Ok(highest(&wasm_releases(content, repo)?, true).cloned())
}

/// The highest-precedence WebAssembly stable release on a releases feed.
pub fn highest_stable(
    content: &[u8],
    repo: &str,
) -> Result<Option<PluginFeedRelease>, PluginReleaseFeedError> {
    Ok(highest(&wasm_releases(content, repo)?, false).cloned())
}

// --- Resolving a repository ----------------------------------------------------

/// A resolved release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginReleaseResolution {
    /// Its tag.
    pub tag: String,
    /// Its `plugin.json` bytes.
    pub manifest_bytes: Vec<u8>,
    /// Its parsed manifest.
    pub manifest: PluginManifest,
}

/// How resolving went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginReleaseResolveResult {
    /// A release to offer.
    Success(Box<PluginReleaseResolution>),
    /// GitHub is rate limiting.
    RateLimited,
    /// Unreachable, not found, or no release for this client.
    Unavailable,
    /// The release is unusable; the text says why.
    Invalid(String),
    /// The repository's newest release for this client is a pre-release
    /// and it has no stable one (not offered on stable).
    Prerelease(String),
}

impl PluginReleaseResolveResult {
    /// The resolution, when successful.
    pub fn resolution(&self) -> Option<&PluginReleaseResolution> {
        match self {
            Self::Success(resolution) => Some(resolution.as_ref()),
            _ => None,
        }
    }
}

/// Both candidates of one pass: the stable release (always) and the highest
/// pre-release (only on the beta channel).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginReleaseCandidates {
    /// The stable release.
    pub stable: PluginReleaseResolveResult,
    /// The beta candidate, when asked for and found.
    pub beta: Option<PluginReleaseResolveResult>,
}

impl PluginReleaseCandidates {
    /// The release a channel offers: stable for stable; for beta, the
    /// higher by precedence of the two, and a failed stable never hides a
    /// beta that resolved.
    pub fn for_channel(&self, channel: PluginReleaseChannel) -> &PluginReleaseResolveResult {
        if channel != PluginReleaseChannel::Beta {
            return &self.stable;
        }
        let Some(beta) = &self.beta else {
            return &self.stable;
        };
        let (Some(stable), Some(beta_resolution)) = (self.stable.resolution(), beta.resolution())
        else {
            return if self.stable.resolution().is_none() {
                beta
            } else {
                &self.stable
            };
        };
        let stable_version = LauncherVersion::try_parse(&stable.manifest.version);
        let beta_version = LauncherVersion::try_parse(&beta_resolution.manifest.version);
        match (stable_version, beta_version) {
            (Some(stable_version), Some(beta_version)) if beta_version > stable_version => beta,
            _ => &self.stable,
        }
    }
}

/// Resolves repositories to releases. Both channels come from one fetch of
/// the repository's releases feed: the highest `wasm-v<version>` stable
/// release, and on beta the highest `wasm-v<version>` pre-release. A
/// repository's
/// `releases/latest` belongs to OpenAC's C# launcher and is never read.
#[derive(Debug, Clone)]
pub struct PluginReleaseResolver {
    client: PluginReleaseClient,
}

impl PluginReleaseResolver {
    /// A resolver over `client`.
    pub fn new(client: PluginReleaseClient) -> Self {
        Self { client }
    }

    /// Resolves the stable release and, on beta, the highest pre-release
    /// too.
    pub fn resolve_candidates(
        &self,
        repo: &str,
        channel: PluginReleaseChannel,
        cancel: &CancelToken,
    ) -> Result<PluginReleaseCandidates, UpdateError> {
        let feed_url = github::releases_feed(repo).ok_or_else(|| {
            UpdateError::failed(format!("'{repo}' is not an 'owner/name' repository."))
        })?;
        let only = |stable| Ok(PluginReleaseCandidates { stable, beta: None });
        let feed = match self
            .client
            .fetch_document(&feed_url, FEED_MAXIMUM_BYTES, cancel)
        {
            Ok(PluginReleaseFetchResult::Success(document)) => document,
            Ok(PluginReleaseFetchResult::RateLimited) => {
                return only(PluginReleaseResolveResult::RateLimited);
            }
            Ok(PluginReleaseFetchResult::Unavailable) => {
                return only(PluginReleaseResolveResult::Unavailable);
            }
            Err(UpdateError::Cancelled) => return Err(UpdateError::Cancelled),
            // An oversized feed or a redirect off HTTPS: no release can be read from it.
            Err(_) => return only(PluginReleaseResolveResult::Unavailable),
        };
        let releases = match wasm_releases(&feed.content, repo) {
            Ok(releases) => releases,
            Err(error) => return only(PluginReleaseResolveResult::Invalid(error.to_string())),
        };
        let stable = match (highest(&releases, false), highest(&releases, true)) {
            (Some(release), _) => self.resolve_tag(repo, &release.tag, cancel)?,
            (None, Some(pre_release)) => PluginReleaseResolveResult::Prerelease(format!(
                "the latest release of '{repo}' is a pre-release ({})",
                pre_release.version.value()
            )),
            (None, None) => PluginReleaseResolveResult::Unavailable,
        };
        let beta = match highest(&releases, true) {
            Some(release) if channel == PluginReleaseChannel::Beta => {
                // A beta that cannot be read is simply not offered.
                match self.resolve_tag(repo, &release.tag, cancel) {
                    Ok(result @ PluginReleaseResolveResult::Success(_)) => Some(result),
                    Ok(_) => None,
                    Err(UpdateError::Cancelled) => return Err(UpdateError::Cancelled),
                    Err(_) => None,
                }
            }
            _ => None,
        };
        Ok(PluginReleaseCandidates { stable, beta })
    }

    /// The release the channel offers.
    pub fn resolve(
        &self,
        repo: &str,
        channel: PluginReleaseChannel,
        cancel: &CancelToken,
    ) -> Result<PluginReleaseResolveResult, UpdateError> {
        Ok(self
            .resolve_candidates(repo, channel, cancel)?
            .for_channel(channel)
            .clone())
    }

    /// The release tagged `tag`: its `plugin.json`, read by the client's
    /// parser, naming the tag's version.
    fn resolve_tag(
        &self,
        repo: &str,
        tag: &str,
        cancel: &CancelToken,
    ) -> Result<PluginReleaseResolveResult, UpdateError> {
        let Some(url) = github::tagged_asset(repo, tag, "plugin.json") else {
            return Ok(PluginReleaseResolveResult::Unavailable);
        };
        let document = match self
            .client
            .fetch_document(&url, MAXIMUM_DOCUMENT_BYTES, cancel)?
        {
            PluginReleaseFetchResult::Success(document) => document,
            PluginReleaseFetchResult::RateLimited => {
                return Ok(PluginReleaseResolveResult::RateLimited);
            }
            PluginReleaseFetchResult::Unavailable => {
                return Ok(PluginReleaseResolveResult::Unavailable);
            }
        };
        let content = document
            .content
            .strip_prefix(b"\xEF\xBB\xBF")
            .unwrap_or(&document.content);
        let manifest = match PluginManifest::parse(&String::from_utf8_lossy(content)) {
            Ok(manifest) => manifest,
            Err(error) => return Ok(PluginReleaseResolveResult::Invalid(error.to_string())),
        };
        if !manifest.matches_tag(tag) {
            return Ok(PluginReleaseResolveResult::Invalid(format!(
                "the release tag does not match plugin version {}",
                manifest.version
            )));
        }
        Ok(PluginReleaseResolveResult::Success(Box::new(
            PluginReleaseResolution {
                tag: tag.to_owned(),
                manifest_bytes: document.content,
                manifest,
            },
        )))
    }
}
