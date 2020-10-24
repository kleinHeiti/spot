use isahc::prelude::*;
use serde_json::from_str;
use std::convert::Into;
use std::cell::RefCell;
use futures::future::LocalBoxFuture;

use crate::app::{SongDescription, AlbumDescription};
use super::cache::{CacheManager, CacheFile, CachePolicy};
use super::api_models::*;

const SPOTIFY_API: &'static str = "https://api.spotify.com/v1";

pub trait SpotifyApiClient {
    fn get_album(&self, id: &str) -> LocalBoxFuture<Option<Vec<SongDescription>>>;
    fn get_saved_albums(&self) -> LocalBoxFuture<Option<Vec<AlbumDescription>>>;
    fn update_token(&self, token: &str);
}

pub struct CachedSpotifyClient {
    token: RefCell<Option<String>>,
    cache: CacheManager
}

const ME_ALBUMS_CACHE: &'static str = "me_albums.json";

impl CachedSpotifyClient {

    pub fn new() -> CachedSpotifyClient {
        CachedSpotifyClient { token: RefCell::new(None), cache: CacheManager::new() }
    }

    async fn get_cache_for(&self, resource: &str) -> Option<String> {
        let policy = if self.token.borrow().is_some() {
            CachePolicy::Default
        } else {
            CachePolicy::IgnoreExpiry
        };
        match self.cache.read_cache_file(resource, policy).await {
            CacheFile::File(buffer) => String::from_utf8(buffer).ok(),
            _ => None
        }
    }

    async fn get_album_no_cache(&self, id: &str) -> Option<String> {

        let token = self.token.borrow();
        let token = token.as_deref()?;

        let uri = format!("{}/albums/{}", SPOTIFY_API, id);
        let request = Request::get(uri)
            .header("Authorization", format!("Bearer {}", token))
            .body(())
            .unwrap();

        let mut result = request.send_async().await.ok()?;
        result.text_async().await.ok()
    }

    async fn get_saved_albums_no_cache(&self) -> Option<String> {

        let token = self.token.borrow();
        let token = token.as_deref()?;

        let uri = format!("{}/me/albums", SPOTIFY_API);
        let request = Request::get(uri)
            .header("Authorization", format!("Bearer {}", token))
            .body(())
            .unwrap();

        let mut result = request.send_async().await.ok()?;
        result.text_async().await.ok()
    }
}

impl SpotifyApiClient for CachedSpotifyClient {

    fn update_token(&self, token: &str) {
        self.token.replace(Some(token.to_string()));
    }

    fn get_saved_albums(&self) -> LocalBoxFuture<Option<Vec<AlbumDescription>>> {
        Box::pin(async move {
            let text = self.get_cache_for(ME_ALBUMS_CACHE).await;
            let text = match text {
                Some(text) => text,
                None => {
                    let response = self.get_saved_albums_no_cache().await?;
                    self.cache.write_cache_file(ME_ALBUMS_CACHE, response.as_bytes()).await?;
                    response
                }
            };

            let page = from_str::<Page<SavedAlbum>>(&text).ok()?;

            Some(page.items.iter()
                .map(|saved| saved.album.clone().into())
                .collect::<Vec<AlbumDescription>>())
        })
    }


    fn get_album(&self, id: &str) -> LocalBoxFuture<Option<Vec<SongDescription>>> {

        let id = id.to_owned();

        Box::pin(async move {
            let key = format!("album_{}.json", id);

            let text = self.get_cache_for(&key[..]).await;
            let text = match text {
                Some(text) => text,
                None => {
                    let response = self.get_album_no_cache(&id[..]).await?;
                    self.cache.write_cache_file(&key[..], response.as_bytes()).await?;
                    response
                }
            };

            Some(from_str::<Album>(&text).ok()?.into())
        })
    }
}