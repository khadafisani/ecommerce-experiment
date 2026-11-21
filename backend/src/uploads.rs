use crate::{Error, Result};
use axum::{
    body::{Body, Bytes},
    extract::Request,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use img4avif::{Config, ConversionOutput, Converter, OutputResolution};
use serde::Deserialize;
use std::{
    convert::Infallible,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;
use tokio::sync::Semaphore;
use tower::{Service, ServiceExt, service_fn};
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

pub const PUBLIC_DIR: &str = "uploads";
pub const PRIVATE_DIR: &str = "private-uploads";
pub const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;
static IMAGE_ENCODERS: Semaphore = Semaphore::const_new(2);

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageSize {
    Sm,
    Md,
    Lg,
    Xlg,
}

impl ImageSize {
    const ALL: [Self; 4] = [Self::Sm, Self::Md, Self::Lg, Self::Xlg];

    fn suffix(self) -> &'static str {
        match self {
            Self::Sm => "sm",
            Self::Md => "md",
            Self::Lg => "lg",
            Self::Xlg => "xlg",
        }
    }

    fn resolution(self) -> OutputResolution {
        let width = match self {
            Self::Sm => 320,
            Self::Md => 640,
            Self::Lg => 960,
            Self::Xlg => 1440,
        };
        OutputResolution::Custom(width)
    }

    fn filename(self, stem: &str) -> String {
        format!("{stem}-{}.avif", self.suffix())
    }
}

pub async fn save_image(directory: &str, stem: String, bytes: Bytes) -> Result<()> {
    let permit = IMAGE_ENCODERS.try_acquire().map_err(|_| Error::busy())?;
    let directory = PathBuf::from(directory);
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let resolutions = ImageSize::ALL
            .map(ImageSize::resolution)
            .into_iter()
            .chain([OutputResolution::Original])
            .collect();
        let config = Config::default()
            .max_pixels(20_000_000)
            .max_input_bytes(MAX_FILE_BYTES as u64)
            .output_resolutions(resolutions);
        let images = Converter::new(config)
            .and_then(|converter| converter.convert_multi(&bytes))
            .map_err(|error| {
                tracing::debug!(%error, "image conversion failed");
                Error::bad("This image could not be converted to AVIF")
            })?;
        publish_images(&directory, &stem, images).map_err(Error::internal)
    })
    .await
    .map_err(Error::internal)?
}

fn publish_images(directory: &Path, stem: &str, images: Vec<ConversionOutput>) -> io::Result<()> {
    // Write complete temp files first and publish the original URL last.
    let names = ImageSize::ALL
        .map(|size| size.filename(stem))
        .into_iter()
        .chain([format!("{stem}.avif")]);
    let staged = names
        .zip(images)
        .map(|(name, image)| {
            let mut file = NamedTempFile::new_in(directory)?;
            file.write_all(&image.data)?;
            Ok((file, directory.join(name)))
        })
        .collect::<io::Result<Vec<_>>>()?;

    let mut published = Vec::with_capacity(staged.len());
    for (file, path) in staged {
        if let Err(error) = file.persist_noclobber(&path) {
            for path in published {
                if let Err(cleanup_error) = fs::remove_file(&path) {
                    tracing::warn!(path = ?path, %cleanup_error, "could not remove incomplete image upload");
                }
            }
            return Err(error.error);
        }
        published.push(path);
    }
    Ok(())
}

pub fn public_files(
    directory: impl Into<PathBuf>,
) -> impl Service<Request, Response = Response, Error = Infallible, Future: Send + 'static>
+ Clone
+ Send
+ Sync {
    let directory = directory.into();
    let files = ServeDir::new(&directory).fallback(service_fn(move |request| {
        serve_original(directory.clone(), request)
    }));
    service_fn(move |request: Request| {
        let files = files.clone();
        async move {
            if !public_filename(request.uri().path()) {
                return Ok(StatusCode::NOT_FOUND.into_response());
            }
            let response = files.oneshot(request).await?;
            Ok(response.map(Body::new))
        }
    })
}

fn public_filename(path: &str) -> bool {
    let Some((stem, extension)) = path
        .strip_prefix('/')
        .and_then(|name| name.rsplit_once('.'))
    else {
        return false;
    };
    matches!(extension, "avif" | "mp4" | "webm")
        && (Uuid::parse_str(stem).is_ok() || original_stem(path).is_some())
}

async fn serve_original(
    directory: PathBuf,
    request: Request,
) -> std::result::Result<Response, Infallible> {
    let Some(stem) = original_stem(request.uri().path()) else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let path = directory.join(format!("{stem}.avif"));
    let response = ServeFile::new(path).oneshot(request).await?;
    Ok(response.map(Body::new))
}

fn original_stem(path: &str) -> Option<&str> {
    let name = path.strip_prefix('/')?.strip_suffix(".avif")?;
    let (stem, suffix) = name.rsplit_once('-')?;
    (ImageSize::ALL.iter().any(|size| size.suffix() == suffix) && Uuid::parse_str(stem).is_ok())
        .then_some(stem)
}

pub async fn serve_private(
    name: &str,
    size: Option<ImageSize>,
    request: Request,
) -> Result<Response> {
    let directory = Path::new(PRIVATE_DIR);
    let mut path = directory.join(name);
    if let (Some(stem), Some(size)) = (name.strip_suffix(".avif"), size) {
        let variant = directory.join(size.filename(stem));
        if tokio::fs::try_exists(&variant)
            .await
            .map_err(Error::internal)?
        {
            path = variant;
        }
    }
    let Ok(response) = ServeFile::new(path).oneshot(request).await;
    Ok(response.map(Body::new))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::to_bytes, http::header};

    #[tokio::test]
    async fn serves_an_existing_variant_and_falls_back_for_legacy_uploads() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let stem = Uuid::new_v4().to_string();
        fs::write(directory.path().join(format!("{stem}.avif")), b"original")?;
        fs::write(directory.path().join(format!("{stem}-sm.avif")), b"small")?;
        fs::write(directory.path().join("staging-file"), b"unpublished")?;
        let router =
            Router::new().nest_service("/uploads", public_files(directory.path().to_path_buf()));

        for (suffix, contents) in [
            (".avif", b"original".as_slice()),
            ("-sm.avif", b"small".as_slice()),
            ("-md.avif", b"original".as_slice()),
            ("-lg.avif", b"original".as_slice()),
            ("-xlg.avif", b"original".as_slice()),
        ] {
            let request = Request::builder()
                .uri(format!("/uploads/{stem}{suffix}"))
                .body(Body::empty())?;
            let response = router.clone().oneshot(request).await?;
            assert_eq!(response.status(), StatusCode::OK, "{suffix}");
            assert_eq!(response.headers()[header::CONTENT_TYPE], "image/avif");
            assert_eq!(to_bytes(response.into_body(), 32).await?, contents);
        }
        let request = Request::builder()
            .uri(format!("/uploads/{stem}-unknown.avif"))
            .body(Body::empty())?;
        assert_eq!(
            router.clone().oneshot(request).await?.status(),
            StatusCode::NOT_FOUND
        );
        let unpublished = Request::builder()
            .uri("/uploads/staging-file")
            .body(Body::empty())?;
        assert_eq!(
            router.oneshot(unpublished).await?.status(),
            StatusCode::NOT_FOUND
        );
        Ok(())
    }
}
