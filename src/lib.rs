use chrono::NaiveDateTime;
use gtfs_realtime::{
    feed_header::Incrementality, FeedEntity, FeedHeader,
    FeedMessage, Alert, EntitySelector,
    alert::{Cause, Effect},
    TimeRange,
    translated_string::Translation,
};
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Deserialize;
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Deserialize, Debug)]
struct PathResponse {
    #[serde(rename = "Content")]
    content: String,
}

pub async fn fetch_path_alerts() -> Result<FeedMessage, Box<dyn Error>> {
    let client = Client::new();
    let url = "https://path-mppprod-app.azurewebsites.net/api/v1/AppContent/fetch?contentKey=PathAlert";
    
    let resp = client.get(url).send().await?.json::<PathResponse>().await?;
    parse_path_alerts(&resp.content)
}

use regex::Regex;
use std::sync::LazyLock;

static STATION_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse("div.station").unwrap());
static DATE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse("div.stationName table tr td strong span").unwrap());
static TEXT_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse("span.alertText").unwrap());
static APOLOGIZE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"We (apologize|regret) (for )?(the|this|any)?( )?(inconvenience)( )?(this )?(may )?(have|has)?( )?(caused)?(.*\.?)").unwrap());

pub fn parse_path_alerts(content: &str) -> Result<FeedMessage, Box<dyn Error>> {
    let clean_content = content.replace("&quot", "\"");
    let document = Html::parse_document(&clean_content);

    let mut entities = Vec::new();
    let current_timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    for (index, element) in document.select(&STATION_SELECTOR).enumerate() {
        let mut date_str = String::new();
        let mut time_str = String::new();
        
        // Extract date and time
        let mut date_time_iter = element.select(&DATE_SELECTOR);
        if let Some(d) = date_time_iter.next() {
            date_str = d.text().collect::<Vec<_>>().join("").trim().to_string();
        }
        if let Some(t) = date_time_iter.next() {
            time_str = t.text().collect::<Vec<_>>().join("").trim().to_string();
        }

        let full_date_str = format!("{} {}", date_str, time_str);
        // Format: 11/25/2025 11:21 PM
        let parsed_time = NaiveDateTime::parse_from_str(&full_date_str, "%m/%d/%Y %I:%M %p");
        
        let timestamp = match parsed_time {
            Ok(dt) => {
                // Assuming Eastern Time (New York) - simplified
                 dt.and_utc().timestamp() as u64
            },
            Err(_) => current_timestamp, // Fallback
        };

        let mut alert_text = String::new();
        if let Some(text_el) = element.select(&TEXT_SELECTOR).next() {
            alert_text = text_el.text().collect::<Vec<_>>().join("").trim().to_string();
        }

        // Clean up alert text
        let clean_alert_text = APOLOGIZE_REGEX.replace_all(&alert_text, "").trim().to_string();

        if clean_alert_text.is_empty() {
            continue;
        }

        let entity = FeedEntity {
            id: format!("path_alert_{}", index),
            is_deleted: None,
            trip_update: None,
            vehicle: None,
            alert: Some(Alert {
                active_period: vec![TimeRange {
                    start: Some(timestamp),
                    end: None,
                }],
                informed_entity: vec![EntitySelector {
                    agency_id: Some("PATH".to_string()),
                    ..Default::default()
                }],
                cause: Some(Cause::UnknownCause as i32),
                effect: Some(Effect::UnknownEffect as i32),
                url: None,
                header_text: None,
                description_text: Some(gtfs_realtime::TranslatedString {
                    translation: vec![Translation {
                        text: clean_alert_text,
                        language: Some("en".to_string()),
                    }],
                }),
                ..Default::default()
            }),
            shape: None,
            stop: None,
            trip_modifications: None,
        };
        entities.push(entity);
    }

    Ok(FeedMessage {
        header: FeedHeader {
            gtfs_realtime_version: "2.0".to_string(),
            incrementality: Some(Incrementality::FullDataset as i32),
            timestamp: Some(current_timestamp),
            feed_version: Some("1.0".to_string()),
        },
        entity: entities,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_parse_example() {
        let content = fs::read_to_string("example.json").expect("Failed to read example.json");
        let response: PathResponse = serde_json::from_str(&content).expect("Failed to parse JSON");
        
        let feed = parse_path_alerts(&response.content).expect("Failed to parse alerts");
        
        assert!(!feed.entity.is_empty(), "Should have found alerts");
        println!("Found {} alerts", feed.entity.len());
        
        for entity in feed.entity {
            if let Some(alert) = entity.alert {
                if let Some(desc) = alert.description_text {
                    if let Some(trans) = desc.translation.first() {
                        println!("Alert: {}", trans.text);
                        assert!(!trans.text.is_empty());
                    }
                }
            }
        }
    }
}
