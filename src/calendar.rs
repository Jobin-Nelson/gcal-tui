use chrono::{Datelike, Duration, Local};
use futures::stream::{self, StreamExt, TryStreamExt};
use google_calendar3::{
    CalendarHub,
    api::Event,
    hyper_rustls::{HttpsConnector, HttpsConnectorBuilder},
    hyper_util::{
        self,
        client::legacy::{Client, connect::HttpConnector},
    },
    yup_oauth2::{
        self, InstalledFlowAuthenticator, InstalledFlowReturnMethod, read_application_secret,
    },
};

use crate::Result;

type Hub = CalendarHub<HttpsConnector<HttpConnector>>;

pub struct Calendar {
    hub: Hub,
    calendar_ids: Vec<String>,
}

impl Calendar {
    pub async fn new(calendar_ids: Vec<String>) -> Result<Self> {
        // 1. Load the client_secret.json you downloaded from Google Cloud
        let secret = read_application_secret("client_secret.json").await?;

        // 2. Set up the OAuth2 authenticator
        let connector = HttpsConnectorBuilder::new()
            .with_native_roots()
            .unwrap()
            .https_only()
            .enable_http2()
            .build();
        let executor = hyper_util::rt::TokioExecutor::new();
        // This will open a browser window the first time you run it to grant permissions.
        // The token is then cached locally in "tokencache.json".
        let auth = InstalledFlowAuthenticator::with_client(
            secret,
            InstalledFlowReturnMethod::HTTPRedirect,
            yup_oauth2::client::CustomHyperClientBuilder::from(
                Client::builder(executor.clone()).build(connector),
            ),
        )
        .persist_tokens_to_disk("tokencache.json")
        .build()
        .await?;

        let mandatory_scope = &["https://www.googleapis.com/auth/calendar"];
        auth.token(mandatory_scope)
            .await
            .expect("Failed to seed token cache");

        // 3. Initialize the Calendar Hub client
        let client = Client::builder(executor).build(
            HttpsConnectorBuilder::new()
                .with_native_roots()
                .unwrap()
                .https_or_http()
                .enable_http2()
                .build(),
        );
        Ok(Calendar {
            hub: CalendarHub::new(client, auth),
            calendar_ids,
        })
    }

    pub async fn get_events(&self) -> Result<Vec<Event>> {
        // 4. Calculate the start and end of the current week (Monday to Sunday)
        let now = Local::now();
        let days_since_monday = now.weekday().num_days_from_monday() as i64;

        // Set to midnight of the current Monday
        let start_of_week = (now - Duration::days(days_since_monday))
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .unwrap();

        let end_of_week = start_of_week + Duration::days(7);

        // Google Calendar API requires dates in RFC3339 format
        let time_min = start_of_week.to_rfc3339();
        let time_max = end_of_week.to_rfc3339();

        println!("Fetching events from {} to {}...\n", time_min, time_max);

        stream::iter(self.calendar_ids.iter().map(|cal_id| async move {
            let (_, event_list) = self
                .hub
                .events()
                .list(cal_id)
                .time_min(start_of_week.to_utc())
                .time_max(end_of_week.to_utc())
                .single_events(true) // Crucial: expands recurring events into individual instances
                .order_by("startTime") // Returns them chronologically
                .doit()
                .await?;
            Ok(event_list.items.unwrap_or_default())
        }))
        .buffer_unordered(5)
        .try_concat()
        .await
    }
}
