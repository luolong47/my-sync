use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use log::{error, info};
use notify::{Config as NotifyConfig, RecommendedWatcher, RecursiveMode, Watcher};
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use quick_xml::de::from_str as from_xml_str;
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, State, WindowEvent,
};
#[cfg(desktop)]
use tauri_plugin_autostart::ManagerExt;
use tokio::{sync::Mutex, time::sleep};
use url::Url;

include!("models.rs");
include!("webdav.rs");
include!("commands.rs");
include!("sync.rs");
include!("storage.rs");
include!("background.rs");
