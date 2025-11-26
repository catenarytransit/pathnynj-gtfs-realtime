use gtfs_structures::Gtfs;
use pathnynj_gtfs_realtime::fetch_path_alerts;

#[tokio::main]
async fn main() {
    let gtfs = tokio::task::spawn_blocking(|| {
        Gtfs::from_url("http://data.trilliumtransit.com/gtfs/path-nj-us/path-nj-us.zip")
            .expect("Failed to load GTFS")
    })
    .await
    .expect("Failed to join blocking task");

    match fetch_path_alerts(&gtfs).await {
        Ok(feed) => {
            println!("Successfully fetched {} alerts", feed.entity.len());
            for entity in feed.entity {
                if let Some(alert) = entity.alert {
                    for informed in &alert.informed_entity {
                        println!("Informed Entity: {:?}", informed);
                    }
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
