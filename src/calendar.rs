use chrono::{Local, NaiveDate};
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

#[derive(Clone)]
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

    pub async fn get_events(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<Event>> {
        // 4. Calculate the start and end of the current week (Monday to Sunday)
        let start_time = start_date
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .unwrap();
        let end_time = end_date
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .unwrap();

        let cal_ids = self.calendar_ids.clone();
        stream::iter(cal_ids.into_iter().map(|cal_id| {
            let hub_clone = self.hub.clone();
            async move {
                let (_, event_list) = hub_clone
                    .events()
                    .list(&cal_id)
                    .time_min(start_time.to_utc())
                    .time_max(end_time.to_utc())
                    .single_events(true) // Crucial: expands recurring events into individual instances
                    .order_by("startTime") // Returns them chronologically
                    .doit()
                    .await?;
                Ok(event_list.items.unwrap_or_default())
            }
        }))
        .buffer_unordered(5)
        .try_concat()
        .await
    }
}
