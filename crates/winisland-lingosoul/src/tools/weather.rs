use serde_json::{Value, json};

pub(super) async fn get(
    client: &reqwest::Client,
    city: &str,
    language: &str,
) -> Result<Value, String> {
    let language = if language.starts_with("zh") {
        "zh"
    } else {
        "en"
    };
    let (place, latitude, longitude, source) = if city.trim().is_empty() {
        let location = request(
            client,
            "https://ipwho.is/",
            &[("fields", "success,city,latitude,longitude")],
        )
        .await?;
        if location.get("success").and_then(Value::as_bool) != Some(true) {
            return Err("IP location is unavailable. Ask the user for their city.".into());
        }
        (
            location
                .get("city")
                .and_then(Value::as_str)
                .unwrap_or("Approximate location")
                .to_owned(),
            coordinate(&location, "latitude")?,
            coordinate(&location, "longitude")?,
            "ip",
        )
    } else {
        let places = request(
            client,
            "https://geocoding-api.open-meteo.com/v1/search",
            &[
                ("name", city.trim()),
                ("count", "1"),
                ("language", language),
            ],
        )
        .await?;
        let location = places
            .pointer("/results/0")
            .ok_or("The city could not be found. Ask for a more specific location.")?;
        (
            location
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(city)
                .to_owned(),
            coordinate(location, "latitude")?,
            coordinate(location, "longitude")?,
            "user",
        )
    };
    let latitude_text = latitude.to_string();
    let longitude_text = longitude.to_string();
    let forecast = request(client, "https://api.open-meteo.com/v1/forecast", &[
        ("latitude", &latitude_text), ("longitude", &longitude_text),
        ("current", "temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,wind_speed_10m"),
        ("daily", "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max"),
        ("timezone", "auto"), ("forecast_days", "3"),
    ]).await?;
    if forecast.get("current").is_none() || forecast.get("daily").is_none() {
        return Err("The weather service returned an incomplete forecast".into());
    }
    Ok(json!({
        "location": {"name": place, "latitude": latitude, "longitude": longitude, "source": source},
        "current": forecast["current"], "daily": forecast["daily"],
        "current_units": forecast["current_units"], "daily_units": forecast["daily_units"],
        "timezone": forecast["timezone"],
    }))
}

fn coordinate(value: &Value, field: &str) -> Result<f64, String> {
    value
        .get(field)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("The location service did not return {field}"))
}

async fn request(
    client: &reqwest::Client,
    url: &str,
    query: &[(&str, &str)],
) -> Result<Value, String> {
    let mut url = reqwest::Url::parse(url).map_err(|error| error.to_string())?;
    url.query_pairs_mut().extend_pairs(query.iter().copied());
    client
        .get(url)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| error.without_url().to_string())?
        .json()
        .await
        .map_err(|error| error.without_url().to_string())
}
