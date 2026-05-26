use chrono::{Datelike, Duration, Local};
use google_calendar3::{
    CalendarHub,
    hyper_rustls::HttpsConnectorBuilder,
    hyper_util::{self, client::legacy::Client},
    yup_oauth2::{
        self, InstalledFlowAuthenticator, InstalledFlowReturnMethod, read_application_secret,
    },
};

#[tokio::main]
async fn main() {
    // 1. Load the client_secret.json you downloaded from Google Cloud
    let secret = read_application_secret("client_secret.json")
        .await
        .expect("Failed to read client_secret.json. Ensure the file is in your project root.");

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
    .await
    .expect("Failed to build authenticator");

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
    let hub = CalendarHub::new(client, auth);

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

    // let (_, calendar_list) = hub
    //     .calendar_list()
    //     .list()
    //     .doit()
    //     .await
    //     .expect("Failed to fetch calendar list");
    //
    // let calendars = calendar_list.items.unwrap_or_default();
    // if calendars.is_empty() {
    //     println!("No Calendars found");
    // }

    let calendar_ids = [
        "jobinnelson369@gmail.com",
        "3nvob228pjf3tqguev5a42vqus@group.calendar.google.com",
        "a1gib8smdet8ljbkfe1ohhmsr0@group.calendar.google.com",
        "fk5ttnq95q2oabjfjdirm2pcho@group.calendar.google.com",
    ];

    for cal_id in calendar_ids {
        println!("{}", cal_id);
        // 5. Query the Calendar API
        let (_, event_list) = hub
            .events()
            .list(cal_id)
            .time_min(start_of_week.to_utc())
            .time_max(end_of_week.to_utc())
            .single_events(true) // Crucial: expands recurring events into individual instances
            .order_by("startTime") // Returns them chronologically
            .doit()
            .await
            .expect("Error: Fetching events ");

        // 6. Parse and print the terminal output
        let events = event_list.items.unwrap_or_default();
        if events.is_empty() {
            println!("No events found for this week.");
            continue;
        }

        for event in events {
            let title = event.summary.unwrap_or_else(|| "(No Title)".to_string());

            // Google Calendar events either have a specific `date_time` or just a `date` (for all-day events)
            let start = event.start.as_ref().and_then(|s| s.date_time.as_ref());
            let start_date = event.start.as_ref().and_then(|s| s.date.as_ref());

            let end = event.end.as_ref().and_then(|e| e.date_time.as_ref());

            println!("Event        : {}", title);
            println!("  Start Date : {:?}", &start);
            println!("  Start Time : {:?}", &start_date);
            println!("  End        : {:?}\n", end);
        }
    }
}
