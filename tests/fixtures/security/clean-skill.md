---
name: weather-lookup
version: "1.0"
description: Get current weather for any location
---

# Weather Lookup

Get current weather conditions for any city worldwide.

## Usage

Ask for the weather in any city:

- "What's the weather in Tokyo?"
- "Current conditions in London"
- "Temperature in New York"

## How It Works

The skill uses the OpenWeatherMap free API tier to fetch current conditions.

```python
import requests

def get_weather(city: str) -> dict:
    """Fetch current weather for a city."""
    url = f"https://api.openweathermap.org/data/2.5/weather?q={city}&appid={{API_KEY}}&units=metric"
    response = requests.get(url)
    response.raise_for_status()
    data = response.json()
    return {
        "city": data["name"],
        "temp": data["main"]["temp"],
        "description": data["weather"][0]["description"],
        "humidity": data["main"]["humidity"],
    }
```

## Response Format

The skill returns:
- City name
- Temperature in Celsius
- Weather description (e.g., "clear sky", "light rain")
- Humidity percentage

## Limitations

- Requires an OpenWeatherMap API key (free tier available)
- Updates every 10 minutes
- City names must be in English
