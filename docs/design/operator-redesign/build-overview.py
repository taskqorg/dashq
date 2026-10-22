"""Compose the overview from the server specimen and immutable TaskQ assets."""

from pathlib import Path
import hashlib
import json

DESIGN = Path(__file__).resolve().parent
VENDOR = DESIGN / "vendor" / "taskq" / "v4.6.30"


def upstream(name):
    return (VENDOR / name).read_bytes()


provenance = json.loads(upstream("provenance.json"))
assets = {}
for name in ("project-blocks.js", "project-blocks.css", "answer-card.js", "answer-card.css", "task-controls.js", "task-controls-fixtures.json"):
    data = upstream(name)
    assert hashlib.sha256(data).hexdigest() == provenance["files"][name], name
    assets[name] = data.decode()

server = (DESIGN / "server-specimen.html").read_text()
server = server.replace("id:'normal',", "id:'normal',name:'Mac',")
server = server.replace("id:'warning',", "id:'warning',name:'FRA',")
server = server.replace("id:'stale',", "id:'stale',name:'Desktop',")
server = server.replace('aria-label="FRA · ${s.label}"', 'aria-label="${s.name || \'FRA\'} · ${s.label}"')
server = server.replace('class="ds-name">FRA</h3>', 'class="ds-name">${s.name || \'FRA\'}</h3>')
server = server.replace("host:{name:'FRA'", "host:{name:s.name || 'FRA'")
server = server.replace("${icon('arrow-left')}FRA</button>", "${icon('arrow-left')}${s.name || 'FRA'}</button>")

server = server.replace("saved?.v===1", "saved?.v===2").replace("next?.v!==1", "next?.v!==2")

# Keep the composite preview's host disclosure state separate from the specimen.
server = server.replace("window.openai?.widgetState?.privateContent", "window.openai?.widgetState?.privateContent?.overviewServers")
server = server.replace("privateContent:{v:1,panels}", "privateContent:{...(window.openai?.widgetState?.privateContent||{}),overviewServers:{v:2,panels}}")
server = server.replace("widgetState?.privateContent;if(next", "widgetState?.privateContent?.overviewServers;if(next")

source = (DESIGN / "overview-shell.html").read_text()
license_text = upstream("project-blocks.LICENSE").decode()
source = "<!-- TaskQ " + provenance["release"] + " · " + provenance["source"] + "\n" + license_text + "\n-->\n" + source
source = source.replace("<!-- SERVER_COMPONENT -->", server)
source = source.replace("<!-- DIAGNOSTIC_PAGES -->", (DESIGN / "diagnostics-pages.html").read_text())
source = source.replace("<!-- AGENT_PAGES -->", (DESIGN / "agents-pages.html").read_text())
source = source.replace("// AGENT_LOGIC", (DESIGN / "agents-logic.js").read_text())
source = source.replace("<!-- REPORT_PAGES -->", (DESIGN / "reports-pages.html").read_text())
source = source.replace("// REPORT_LOGIC", (DESIGN / "reports-logic.js").read_text())
source = source.replace("// TASK_CONTROL_POLICY", "const taskControlStates=" + json.dumps(json.loads(assets["task-controls-fixtures.json"])["states"]) + ";")
source = source.replace("<!-- TASKQ_STYLES -->", "<style>\n" + assets["project-blocks.css"] + assets["answer-card.css"] + "\n</style>")
source = source.replace("<!-- TASKQ_SCRIPTS -->", "<script>\n" + assets["project-blocks.js"] + assets["answer-card.js"] + assets["task-controls.js"] + "\n</script>")
assert "<!-- SERVER_COMPONENT -->" not in source
assert "<!-- DIAGNOSTIC_PAGES -->" not in source
assert "<!-- AGENT_PAGES -->" not in source and "// AGENT_LOGIC" not in source
assert "<!-- REPORT_PAGES -->" not in source and "// REPORT_LOGIC" not in source
assert "// TASK_CONTROL_POLICY" not in source
assert "viz-dotted-background" not in source
assert "Last 30 minutes" in source
(DESIGN / "overview-candidate.html").write_text(source)
print("Overview composed; released TaskQ v4.6.30 asset checksums match.")
