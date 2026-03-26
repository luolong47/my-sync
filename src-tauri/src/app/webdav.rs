struct WebDavClient {
    client: reqwest::Client,
    settings: WebDavSettings,
}

impl WebDavClient {
    fn new(settings: WebDavSettings) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .user_agent("my-sync/0.1")
            .build()
            .map_err(|err| err.to_string())?;

        Ok(Self { client, settings })
    }

    async fn fetch_file(&self, remote_path: &str) -> Result<RemoteFile, String> {
        let url = self.file_url(remote_path)?;
        let response = self
            .client
            .get(url)
            .basic_auth(&self.settings.username, Some(&self.settings.password))
            .send()
            .await
            .map_err(|err| err.to_string())?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(RemoteFile {
                bytes: None,
                modified_at: None,
                http_status: Some(StatusCode::NOT_FOUND.as_u16()),
            });
        }

        let status = response.status().as_u16();
        let response = response.error_for_status().map_err(|err| err.to_string())?;
        let modified_at = response
            .headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| httpdate::parse_http_date(value).ok())
            .map(DateTime::<Utc>::from);
        let bytes = response.bytes().await.map_err(|err| err.to_string())?.to_vec();

        Ok(RemoteFile {
            bytes: Some(bytes),
            modified_at,
            http_status: Some(status),
        })
    }

    async fn upload_file(&self, remote_path: &str, bytes: Vec<u8>) -> Result<u16, String> {
        self.ensure_parent_collections(remote_path).await?;
        let url = self.file_url(remote_path)?;
        let response = self
            .client
            .put(url)
            .basic_auth(&self.settings.username, Some(&self.settings.password))
            .body(bytes)
            .send()
            .await
            .map_err(|err| err.to_string())?;

        let status = response.status().as_u16();
        response.error_for_status().map_err(|err| err.to_string())?;
        Ok(status)
    }

    async fn ensure_root_collection(&self) -> Result<(), String> {
        self.ensure_collection_chain(&self.remote_root_segments())
            .await
    }

    async fn create_remote_directory(&self, remote_dir_path: &str) -> Result<(), String> {
        let mut all_segments = self.remote_root_segments();
        all_segments.extend(normalize_segments(remote_dir_path));
        self.ensure_collection_chain(&all_segments).await
    }

    async fn ensure_parent_collections(&self, remote_path: &str) -> Result<(), String> {
        let mut all_segments = self.remote_root_segments();
        let file_segments = normalize_segments(remote_path);

        if file_segments.len() > 1 {
            all_segments.extend(file_segments[..file_segments.len() - 1].iter().cloned());
        }

        self.ensure_collection_chain(&all_segments)
            .await
    }

    async fn list_remote_entries(&self, remote_path: &str) -> Result<Vec<RemoteBrowserEntry>, String> {
        let propfind = Method::from_bytes(b"PROPFIND").map_err(|err| err.to_string())?;
        let url = self.collection_url(&join_remote_segments_with_client(
            &self.settings.remote_dir,
            &self.settings.client_id,
            remote_path,
        ))?;
        let response = self
            .client
            .request(propfind, url.clone())
            .basic_auth(&self.settings.username, Some(&self.settings.password))
            .header("Depth", "1")
            .body(
                r#"<?xml version="1.0" encoding="utf-8"?><propfind xmlns="DAV:"><prop><displayname/><resourcetype/><getcontentlength/><getlastmodified/></prop></propfind>"#,
            )
            .send()
            .await
            .map_err(|err| err.to_string())?;

        let response = response.error_for_status().map_err(|err| err.to_string())?;
        let xml = response.text().await.map_err(|err| err.to_string())?;
        let parsed: MultiStatus = from_xml_str(&xml).map_err(|err| err.to_string())?;
        let current_path = url.path().trim_end_matches('/').to_string();

        let mut entries = Vec::new();
        for item in parsed.responses {
            let href_path = extract_href_path(&item.href)?;
            if href_path.trim_end_matches('/') == current_path.trim_end_matches('/') {
                continue;
            }

            let relative = relative_remote_path(&href_path, &current_path);
            if relative.is_empty() {
                continue;
            }

            if relative.trim_end_matches('/').contains('/') {
                continue;
            }

            let prop = item.propstats.into_iter().next().map(|value| value.prop);
            let display_name = prop
                .as_ref()
                .and_then(|value| value.display_name.clone())
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| relative.clone());
            let is_dir = prop
                .as_ref()
                .and_then(|value| value.resource_type.as_ref())
                .and_then(|value| value.collection.as_ref())
                .is_some()
                || href_path.ends_with('/');
            let size = prop
                .as_ref()
                .and_then(|value| value.content_length.as_ref())
                .and_then(|value| value.parse::<u64>().ok());
            let modified_at = prop
                .as_ref()
                .and_then(|value| value.last_modified.as_ref())
                .and_then(|value| httpdate::parse_http_date(value).ok())
                .map(|value| DateTime::<Utc>::from(value).to_rfc3339());

            entries.push(RemoteBrowserEntry {
                name: display_name,
                path: if remote_path.trim().is_empty() {
                    relative.trim_end_matches('/').to_string()
                } else {
                    format!(
                        "{}/{}",
                        remote_path.trim_matches('/'),
                        relative.trim_end_matches('/')
                    )
                },
                is_dir,
                size,
                modified_at,
            });
        }

        entries.sort_by(|left, right| {
            left.is_dir
                .cmp(&right.is_dir)
                .reverse()
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(entries)
    }

    async fn delete_remote_entry(&self, remote_path: &str) -> Result<(), String> {
        let url = self.file_url(remote_path)?;
        let response = self
            .client
            .delete(url)
            .basic_auth(&self.settings.username, Some(&self.settings.password))
            .send()
            .await
            .map_err(|err| err.to_string())?;

        response.error_for_status().map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn move_remote_entry(
        &self,
        remote_path: &str,
        new_remote_path: &str,
        overwrite: bool,
    ) -> Result<u16, String> {
        let method = Method::from_bytes(b"MOVE").map_err(|err| err.to_string())?;
        let source_url = self.file_url(remote_path)?;
        let destination_url = self.file_url(new_remote_path)?;
        let response = self
            .client
            .request(method, source_url)
            .basic_auth(&self.settings.username, Some(&self.settings.password))
            .header("Destination", destination_url.as_str())
            .header("Overwrite", if overwrite { "T" } else { "F" })
            .send()
            .await
            .map_err(|err| err.to_string())?;

        let status = response.status().as_u16();
        response.error_for_status().map_err(|err| err.to_string())?;
        Ok(status)
    }

    async fn ensure_collection_chain(&self, all_segments: &[String]) -> Result<(), String> {
        let mkcol = Method::from_bytes(b"MKCOL").map_err(|err| err.to_string())?;
        let mut progressive: Vec<String> = Vec::new();
        for segment in all_segments {
            progressive.push(segment.clone());
            let url = self.collection_url(&progressive)?;
            let response = self
                .client
                .request(mkcol.clone(), url)
                .basic_auth(&self.settings.username, Some(&self.settings.password))
                .send()
                .await
                .map_err(|err| err.to_string())?;

            let status = response.status();
            if !matches!(
                status,
                StatusCode::CREATED
                    | StatusCode::OK
                    | StatusCode::METHOD_NOT_ALLOWED
                    | StatusCode::CONFLICT
                    | StatusCode::MOVED_PERMANENTLY
                    | StatusCode::FOUND
            ) {
                return Err(format!("无法创建远端目录，状态码 {status}"));
            }
        }

        Ok(())
    }

    fn file_url(&self, remote_path: &str) -> Result<Url, String> {
        self.build_url_with_segments(&join_remote_segments_with_client(
            &self.settings.remote_dir,
            &self.settings.client_id,
            remote_path,
        ))
    }

    fn collection_url(&self, segments: &[String]) -> Result<Url, String> {
        self.build_url_with_segments(segments)
    }

    fn build_url_with_segments(&self, extra_segments: &[String]) -> Result<Url, String> {
        let mut url = Url::parse(self.settings.base_url.trim()).map_err(|err| err.to_string())?;
        let mut path_segments: Vec<String> = url
            .path()
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(|segment| segment.to_string())
            .collect();

        path_segments.extend(extra_segments.iter().cloned());
        let encoded = path_segments
            .iter()
            .map(|segment| utf8_percent_encode(segment, PATH_SEGMENT_ENCODE_SET).to_string())
            .collect::<Vec<_>>()
            .join("/");

        url.set_path(&format!("/{}", encoded));
        Ok(url)
    }

    fn remote_root_segments(&self) -> Vec<String> {
        let mut segments = normalize_segments(&self.settings.remote_dir);
        if !self.settings.client_id.trim().is_empty() {
            segments.push(self.settings.client_id.trim().to_string());
        }
        segments
    }
}

