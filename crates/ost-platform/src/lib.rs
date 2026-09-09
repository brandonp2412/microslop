use anyhow::{Context, Result, bail};
#[cfg(not(target_os = "android"))]
use directories::ProjectDirs;
#[cfg(not(target_os = "android"))]
use keyring::{Entry, Error as KeyringError};
#[cfg(target_os = "android")]
use keyring_core::{Entry, Error as KeyringError};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
#[cfg(target_os = "android")]
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

const SERVICE_NAME: &str = "com.eisbaw.microslop";
const MAX_CREDENTIAL_UTF16_CODE_UNITS: usize = 1_280;
const CHUNK_COUNT_SUFFIX: &str = "::chunk-count";

pub trait SecretStorage: Send + Sync {
    fn has(&self, key: &str) -> Result<bool>;
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn set(&self, key: &str, value: &str) -> Result<()>;
    fn delete(&self, key: &str) -> Result<()>;
}

pub struct PlatformSecretStorage;

pub struct PlatformCacheStorage {
    root: PathBuf,
}

static CACHE_WRITE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "android")]
static ANDROID_CACHE_ROOT: OnceLock<PathBuf> = OnceLock::new();

impl PlatformCacheStorage {
    pub fn new() -> Result<Self> {
        #[cfg(target_os = "android")]
        let root = ANDROID_CACHE_ROOT
            .get()
            .cloned()
            .context("Android cache directory has not been initialized")?;
        #[cfg(not(target_os = "android"))]
        let root = ProjectDirs::from("com", "eisbaw", "Microslop")
            .map(|dirs| dirs.cache_dir().to_owned())
            .context("Could not determine the platform cache directory")?;
        Ok(Self { root })
    }

    #[cfg(test)]
    fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path(&self, key: &str) -> PathBuf {
        self.root.join(encode_key(key))
    }

    pub fn get(&self, key: &str) -> Result<Option<String>> {
        let path = self.path(key);
        match std::fs::read_to_string(&path) {
            Ok(value) => Ok(Some(value)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => {
                Err(error).with_context(|| format!("Could not read cache file {}", path.display()))
            }
        }
    }

    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        std::fs::create_dir_all(&self.root)
            .with_context(|| format!("Could not create cache directory {}", self.root.display()))?;
        let path = self.path(key);
        let sequence = CACHE_WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = temporary_path(&path, sequence);
        std::fs::write(&temporary, value)
            .with_context(|| format!("Could not write cache file {}", temporary.display()))?;
        if let Err(error) = std::fs::rename(&temporary, &path) {
            let _ = std::fs::remove_file(&temporary);
            return Err(error)
                .with_context(|| format!("Could not replace cache file {}", path.display()));
        }
        Ok(())
    }

    pub fn delete(&self, key: &str) -> Result<()> {
        let path = self.path(key);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => {
                Err(error).with_context(|| format!("Could not clear cache file {}", path.display()))
            }
        }
    }

    pub fn delete_prefix(&self, prefix: &str) -> Result<()> {
        let encoded_prefix = encode_key(prefix);
        let entries = match std::fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("Could not inspect cache directory {}", self.root.display())
                });
            }
        };
        for entry in entries {
            let entry = entry.context("Could not inspect cache entry")?;
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(&encoded_prefix)
            {
                std::fs::remove_file(entry.path()).with_context(|| {
                    format!("Could not clear cache file {}", entry.path().display())
                })?;
            }
        }
        Ok(())
    }
}

fn encode_key(key: &str) -> String {
    let mut encoded = String::with_capacity(key.len() * 2);
    for byte in key.bytes() {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

fn temporary_path(path: &Path, sequence: u64) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".tmp-{}-{sequence}", std::process::id()));
    PathBuf::from(name)
}

impl PlatformSecretStorage {
    fn entry_for_service(&self, service: &str, key: &str) -> Result<Entry> {
        #[cfg(target_os = "android")]
        ensure_android_store()?;
        Entry::new(service, key).context("Could not access platform secure storage")
    }

    fn entry(&self, key: &str) -> Result<Entry> {
        self.entry_for_service(SERVICE_NAME, key)
    }

    fn get_entry_for_service(&self, service: &str, key: &str) -> Result<Option<String>> {
        match self.entry_for_service(service, key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(error) => Err(error).context("Could not read platform secure storage"),
        }
    }

    fn get_entry(&self, key: &str) -> Result<Option<String>> {
        self.get_entry_for_service(SERVICE_NAME, key)
    }

    fn delete_entry(&self, key: &str) -> Result<()> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(error) => Err(error).context("Could not clear platform secure storage"),
        }
    }

    fn chunk_count(&self, key: &str) -> Result<Option<usize>> {
        self.get_entry(&format!("{key}{CHUNK_COUNT_SUFFIX}"))?
            .map(|value| {
                value
                    .parse()
                    .context("Could not read platform secure storage")
            })
            .transpose()
    }

    fn legacy_service_name() -> String {
        ["com.eisbaw.", "ost", "-", "flutter"].concat()
    }

    fn get_legacy(&self, key: &str) -> Result<Option<String>> {
        let service = Self::legacy_service_name();
        let chunk_count = self
            .get_entry_for_service(&service, &format!("{key}{CHUNK_COUNT_SUFFIX}"))?
            .map(|value| {
                value
                    .parse::<usize>()
                    .context("Could not read legacy platform secure storage")
            })
            .transpose()?;
        let Some(chunk_count) = chunk_count else {
            return self.get_entry_for_service(&service, key);
        };
        if chunk_count == 0 {
            bail!("Could not read legacy platform secure storage");
        }
        Ok(Some(
            (0..chunk_count)
                .map(|index| {
                    self.get_entry_for_service(&service, &format!("{key}::{index}"))?
                        .context("Could not read legacy platform secure storage")
                })
                .collect::<Result<String>>()?,
        ))
    }
}

impl SecretStorage for PlatformSecretStorage {
    fn has(&self, key: &str) -> Result<bool> {
        Ok(self.get(key)?.is_some())
    }

    fn get(&self, key: &str) -> Result<Option<String>> {
        let current = match self.chunk_count(key)? {
            Some(0) => bail!("Could not read platform secure storage"),
            Some(chunk_count) => Some(
                (0..chunk_count)
                    .map(|index| {
                        self.get_entry(&format!("{key}::{index}"))?
                            .context("Could not read platform secure storage")
                    })
                    .collect::<Result<String>>()?,
            ),
            None => self.get_entry(key)?,
        };
        if current.is_some() {
            return Ok(current);
        }

        let legacy = self.get_legacy(key)?;
        if let Some(value) = legacy.as_deref() {
            self.set(key, value)?;
        }
        Ok(legacy)
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        let old_chunk_count = self.chunk_count(key)?.unwrap_or_default();
        let chunks = split_secret(value);
        if chunks.len() == 1 {
            self.entry(key)?
                .set_password(value)
                .context("Could not write platform secure storage")?;
            self.delete_entry(&format!("{key}{CHUNK_COUNT_SUFFIX}"))?;
        } else {
            for (index, chunk) in chunks.iter().enumerate() {
                self.entry(&format!("{key}::{index}"))?
                    .set_password(chunk)
                    .context("Could not write platform secure storage")?;
            }
            self.entry(&format!("{key}{CHUNK_COUNT_SUFFIX}"))?
                .set_password(&chunks.len().to_string())
                .context("Could not write platform secure storage")?;
            self.delete_entry(key)?;
        }
        for index in chunks.len()..old_chunk_count {
            self.delete_entry(&format!("{key}::{index}"))?;
        }
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<()> {
        let chunk_count = self.chunk_count(key)?.unwrap_or_default();
        self.delete_entry(key)?;
        self.delete_entry(&format!("{key}{CHUNK_COUNT_SUFFIX}"))?;
        for index in 0..chunk_count {
            self.delete_entry(&format!("{key}::{index}"))?;
        }
        Ok(())
    }
}

#[cfg(target_os = "android")]
fn ensure_android_store() -> Result<()> {
    static RESULT: OnceLock<std::result::Result<(), String>> = OnceLock::new();
    let result = RESULT.get_or_init(|| {
        android_native_keyring_store::Store::new()
            .map(|store| keyring_core::set_default_store(store))
            .map_err(|error| error.to_string())
    });
    result
        .as_ref()
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!("Could not initialize Android secure storage: {error}"))
}

#[cfg(target_os = "android")]
pub fn initialize_android_context<'local>(
    env: jni::JNIEnv<'local>,
    context: jni::objects::JObject<'local>,
    cache_root: impl Into<PathBuf>,
) {
    let _ = ANDROID_CACHE_ROOT.set(cache_root.into());
    android_native_keyring_store::Java_io_crates_keyring_Keyring_00024Companion_initializeNdkContext(
        env,
        jni::objects::JObject::null(),
        context,
    );
}

fn split_secret(value: &str) -> Vec<&str> {
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut code_units = 0;

    for (index, character) in value.char_indices() {
        let character_code_units = character.len_utf16();
        if code_units + character_code_units > MAX_CREDENTIAL_UTF16_CODE_UNITS {
            chunks.push(&value[start..index]);
            start = index;
            code_units = 0;
        }
        code_units += character_code_units;
    }
    chunks.push(&value[start..]);
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_storage_round_trips_without_treating_keys_as_paths() {
        let root = std::env::temp_dir().join(format!(
            "microslop-cache-test-{}-{}",
            std::process::id(),
            CACHE_WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let storage = PlatformCacheStorage::at(&root);

        assert_eq!(storage.get("chat/cache").unwrap(), None);
        storage.set("chat/cache", "cached chats").unwrap();
        assert_eq!(
            storage.get("chat/cache").unwrap().as_deref(),
            Some("cached chats")
        );
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        storage.delete("chat/cache").unwrap();
        assert_eq!(storage.get("chat/cache").unwrap(), None);
        storage.delete("chat/cache").unwrap();
        storage.set("message-cache-one", "one").unwrap();
        storage.set("message-cache-two", "two").unwrap();
        storage.set("other-cache", "other").unwrap();
        storage.delete_prefix("message-cache-").unwrap();
        assert_eq!(storage.get("message-cache-one").unwrap(), None);
        assert_eq!(storage.get("message-cache-two").unwrap(), None);
        assert_eq!(
            storage.get("other-cache").unwrap().as_deref(),
            Some("other")
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn splits_long_secrets_without_breaking_utf8_characters() {
        let value = "🧀".repeat(1_000);

        let chunks = split_secret(&value);

        assert_eq!(chunks.concat(), value);
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.encode_utf16().count() <= MAX_CREDENTIAL_UTF16_CODE_UNITS)
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn round_trips_a_secret_larger_than_a_windows_credential_blob() {
        let storage = PlatformSecretStorage;
        let key = "test-large-secret";
        let value = "a".repeat(MAX_CREDENTIAL_UTF16_CODE_UNITS + 1);

        storage.delete(key).unwrap();
        storage.set(key, &value).unwrap();
        assert_eq!(storage.get(key).unwrap(), Some(value));
        storage.delete(key).unwrap();
    }
}
