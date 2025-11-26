use chrono::NaiveDateTime;
use gtfs_realtime::{
    Alert, EntitySelector, FeedEntity, FeedHeader, FeedMessage, TimeRange,
    alert::{Cause, Effect},
    feed_header::Incrementality,
    translated_string::Translation,
};
use gtfs_structures::Gtfs;
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

const ALERTS_URL: &str =
    "https://path-mppprod-app.azurewebsites.net/api/v1/AppContent/fetch?contentKey=PathAlert";

pub async fn fetch_path_alerts(gtfs: &Gtfs) -> Result<FeedMessage, Box<dyn Error>> {
    let client = Client::new();

    let resp = client
        .get(ALERTS_URL)
        .send()
        .await?
        .json::<PathResponse>()
        .await?;
    parse_path_alerts(&resp.content, gtfs)
}

use regex::Regex;
use std::sync::LazyLock;

static STATION_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("div.station").unwrap());
static DATE_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("div.stationName table tr td strong span").unwrap());
static TEXT_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("span.alertText").unwrap());
static APOLOGIZE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"We (apologize|regret) (for )?(the|this|any)?( )?(inconvenience)( )?(this )?(may )?(have|has)?( )?(caused)?(.*\.?)").unwrap()
});

pub fn parse_path_alerts(content: &str, gtfs: &Gtfs) -> Result<FeedMessage, Box<dyn Error>> {
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
            }
            Err(_) => current_timestamp, // Fallback
        };

        let mut alert_text = String::new();
        if let Some(text_el) = element.select(&TEXT_SELECTOR).next() {
            alert_text = text_el
                .text()
                .collect::<Vec<_>>()
                .join("")
                .trim()
                .to_string();
        }

        // Clean up alert text
        let clean_alert_text = APOLOGIZE_REGEX
            .replace_all(&alert_text, "")
            .trim()
            .to_string();

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
                informed_entity: {
                    let route_ids = find_route_ids(&clean_alert_text, gtfs);
                    let agency_id = gtfs
                        .agencies
                        .first()
                        .and_then(|a| a.id.clone())
                        .or_else(|| Some("PATH".to_string()));

                    if route_ids.is_empty() {
                        vec![EntitySelector {
                            agency_id: agency_id.clone(),
                            ..Default::default()
                        }]
                    } else {
                        route_ids
                            .into_iter()
                            .map(|route_id| EntitySelector {
                                agency_id: agency_id.clone(),
                                route_id: Some(route_id),
                                ..Default::default()
                            })
                            .collect()
                    }
                },
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

fn find_route_ids(text: &str, gtfs: &Gtfs) -> Vec<String> {
    let route_map = [
        ("NWK-WTC", "Newark - World Trade Center"),
        ("HOB-WTC", "Hoboken - World Trade Center"),
        ("JSQ-33", "Journal Square - 33rd Street"),
        ("HOB-33", "Hoboken - 33rd Street"),
    ];

    let mut found_routes = Vec::new();

    for (abbr, long_name) in route_map.iter() {
        if text.contains(abbr) {
            // Find route in GTFS with matching long name
            for route in gtfs.routes.values() {
                if route.long_name.as_deref() == Some(long_name) {
                    found_routes.push(route.id.clone());
                }
            }
        }
    }
    found_routes
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_parse_example() {
        let content = fs::read_to_string("example.json").expect("Failed to read example.json");
        let response: PathResponse = serde_json::from_str(&content).expect("Failed to parse JSON");

        let gtfs = Gtfs {
            routes: std::collections::HashMap::new(),
            agencies: vec![],
            stops: std::collections::HashMap::new(),
            trips: std::collections::HashMap::new(),
            calendar: std::collections::HashMap::new(),
            calendar_dates: std::collections::HashMap::new(),
            fare_attributes: std::collections::HashMap::new(),
            fare_rules: std::collections::HashMap::new(),
            feed_info: vec![],
            shapes: std::collections::HashMap::new(),
            read_duration: std::time::Duration::new(0, 0),
        };

        let feed = parse_path_alerts(&response.content, &gtfs).expect("Failed to parse alerts");

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
