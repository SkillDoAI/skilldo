---
name: github-issues
version: "2.0"
description: Create and manage GitHub issues from natural language
---

# GitHub Issues Manager

Create, update, and search GitHub issues using natural language.

## Usage

- "Create an issue for the login bug in repo/name"
- "List open issues assigned to me"
- "Close issue #42 with a comment"

## Authentication

This skill requires a GitHub personal access token with `repo` scope.
Set it as an environment variable: `export GITHUB_TOKEN=your_token_here`

## Implementation

```python
import requests

class GitHubIssues:
    def __init__(self, token: str):
        self.session = requests.Session()
        self.session.headers.update({
            "Authorization": f"token {token}",
            "Accept": "application/vnd.github.v3+json",
        })
        self.base_url = "https://api.github.com"

    def create_issue(self, owner: str, repo: str, title: str, body: str = "") -> dict:
        url = f"{self.base_url}/repos/{owner}/{repo}/issues"
        response = self.session.post(url, json={"title": title, "body": body})
        response.raise_for_status()
        return response.json()

    def list_issues(self, owner: str, repo: str, state: str = "open") -> list:
        url = f"{self.base_url}/repos/{owner}/{repo}/issues"
        response = self.session.get(url, params={"state": state})
        response.raise_for_status()
        return response.json()

    def close_issue(self, owner: str, repo: str, number: int, comment: str = "") -> dict:
        if comment:
            comment_url = f"{self.base_url}/repos/{owner}/{repo}/issues/{number}/comments"
            self.session.post(comment_url, json={"body": comment})
        url = f"{self.base_url}/repos/{owner}/{repo}/issues/{number}"
        response = self.session.patch(url, json={"state": "closed"})
        response.raise_for_status()
        return response.json()
```

## Error Handling

- Invalid token: Returns 401 with helpful message
- Rate limiting: Respects GitHub API rate limits with exponential backoff
- Not found: Clear error messages for invalid repos or issue numbers
