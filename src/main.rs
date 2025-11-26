use pathnynj_gtfs_realtime::fetch_path_alerts;

#[tokio::main]
async fn main() {
    match fetch_path_alerts().await {
        Ok(feed) => {
            println!("Successfully fetched {} alerts", feed.entity.len());
            for entity in feed.entity {
                if let Some(alert) = entity.alert {
                    if let Some(desc) = alert.description_text {
                        for trans in desc.translation {
                            println!("Alert: {}", trans.text);
                        }
                    }
                }
            }
        }
        Err(e) => eprintln!("Error fetching alerts: {}", e),
    }
}
