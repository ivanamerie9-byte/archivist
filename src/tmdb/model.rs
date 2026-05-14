#![allow(dead_code)]

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SearchMovieResponse {
    pub results: Vec<MovieResult>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchTvResponse {
    pub results: Vec<TvResult>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MovieResult {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub original_title: Option<String>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub poster_path: Option<String>,
    #[serde(default)]
    pub overview: String,
    #[serde(default)]
    pub popularity: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TvResult {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub original_name: Option<String>,
    #[serde(default)]
    pub first_air_date: Option<String>,
    #[serde(default)]
    pub poster_path: Option<String>,
    #[serde(default)]
    pub overview: String,
    #[serde(default)]
    pub popularity: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImagesResponse {
    #[serde(default)]
    pub posters: Vec<Image>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Image {
    pub file_path: String,
    #[serde(default)]
    pub vote_average: f64,
    #[serde(default)]
    pub vote_count: u64,
    #[serde(default)]
    pub iso_639_1: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TvDetails {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub first_air_date: Option<String>,
    #[serde(default)]
    pub seasons: Vec<TvSeason>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TvSeason {
    pub id: u64,
    pub season_number: i32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub poster_path: Option<String>,
    #[serde(default)]
    pub episode_count: u32,
}

pub fn extract_year(date: Option<&str>) -> Option<u16> {
    date.and_then(|d| d.get(..4))
        .and_then(|y| y.parse::<u16>().ok())
}
