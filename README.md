# dashq

One dashboard for taskqorg tools: hosts and versions, boards, agent activity and compression, server checks. Decisions live in dashq.md.

## Run

```sh
dashq serve                      # page on http://127.0.0.1:18882 (loopback only)
dashq report --period=7200       # the /data JSON once; --host=, --agent=, --session= filter
dashq selftest                   # fixture rendering, config, one read of each source
dashq --snapshot DIR             # fixture page as DIR/page.html and DIR/page.png (headless Chrome)
dashq service install            # coreq service unit running `dashq service run`
```

Config: `DASHQ_CONFIG`, else `<install>/config.json`, mode 0600:

```json
{
  "langfuse": {"base_url": "https://langfuse.example.com", "project_id": "…", "public_key": "pk-lf-…", "secret_key": "sk-lf-…"},
  "github": {"token": "<read-only token>"},
  "projects": ["taskqorg/taskq", "taskqorg/agentq", "taskqorg/serverq", "taskqorg/coreq", "taskqorg/specq", "taskqorg/dashq"],
  "serverq": {"report": "/home/<user>/.serverq/report.json", "host": "fra/native"},
  "public_host": "dash.example.com"
}
```

## Deployment (D3)

FRA, user service `dashq-dashq.service`, config in `<install>/config.json`. Caddy site, after the Langfuse UI Access guard:

```caddy
dash.ufobe.com {
    route {
        forward_auth 127.0.0.1:9121 {
            uri /verify
            header_up -Upgrade
            header_up -Connection
        }
        reverse_proxy 127.0.0.1:18882
    }
}
```

Cloudflare: proxied DNS record for `dash.ufobe.com`, the host added to the Langfuse Access application, a public bypass for `/.well-known/acme-challenge/*`. Rollback: remove the site block, `systemctl reload caddy`, `dashq service uninstall`.

Fixture: `fixture/input.json` renders to `fixture/page.html` and `fixture/data.json`; after an intended change rerun `DASHQ_UPDATE_FIXTURE=1 cargo test` and review the diff.
