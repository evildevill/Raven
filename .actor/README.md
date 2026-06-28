# Raven Actor on Apify

[![Raven Actor](https://apify.com/actor-badge?actor=YOUR_USERNAME/raven)](https://apify.com/YOUR_USERNAME/raven)

This Actor wraps [Raven](https://github.com/evildevill/Raven) to provide serverless username reconnaissance across social networks in the cloud. It helps you find usernames across multiple social media platforms without installing and running the tool locally.

## Usage

### Apify Console

1. Go to the Apify Actor page
2. Click "Run"
3. Fill in **Usernames to search** (one or more)
4. The Actor runs and outputs results in the default datastore

### Apify CLI

```bash
apify call YOUR_USERNAME/raven --input='{
  "usernames": ["johndoe", "janedoe"]
}'
```

### Using Apify API

```bash
curl --request POST \
  --url "https://api.apify.com/v2/acts/YOUR_USERNAME~raven/run" \
  --header 'Content-Type: application/json' \
  --header 'Authorization: Bearer YOUR_API_TOKEN' \
  --data '{"usernames": ["johndoe"]}'
```

## Input

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `usernames` | array | Yes | List of usernames to search for |

## Output

Dataset records contain:

| Field | Type | Description |
|-------|------|-------------|
| `username` | string | The username that was searched |
| `links` | array | Found profile URLs |

## Resources

- Minimum: 512 MB RAM
- Recommended: 1 GB RAM for multiple usernames
