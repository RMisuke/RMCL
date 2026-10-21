# Dashboard card template

Minimal isolated-UI plugin: one card, two bridge calls, no build step.

```bash
# from the repo root
node plugin-sdk/scripts/cli.mjs validate --dir plugin-sdk/templates/dashboard-card
node plugin-sdk/scripts/cli.mjs pack --dir plugin-sdk/templates/dashboard-card \
  --out /tmp/hello-card.rmcpkg --rmcli ./target/release/rmcli

./target/release/rmcli plugin install --path /tmp/hello-card.rmcpkg
./target/release/rmcli plugin enable --id dev.example.dashboard-card
```

Then open RMCCC → Dashboard → **添加插件卡片** → pick "Hello Card".

What to look at:

| File | Why |
|------|-----|
| `manifest.json` | `entry.ui` + `contributes.dashboardCards` are both required for an iframe card |
| `ui/app.js` | `connect()` + `announce()`; every read goes through `host.request()` |
| `ui/vendor/rmccc-plugin-ui.js` | the SDK runtime, vendored (works without a bundler) |
