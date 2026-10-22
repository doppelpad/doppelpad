# Local HTTP API

Stream decks, macro pads and scripts can drive Doppelpad over HTTP. Turn it on in
*Settings → Connections → Local API (HTTP)*. It is off by default.

- It listens on `127.0.0.1:26761` (the port can be changed). With *Reachable from the network
  too* it listens on every interface, and then every request needs the token.
- The token is shown in the settings. Send it as `Authorization: Bearer <token>` or as
  `?token=<token>` (for clients that cannot set headers, such as a browser's `EventSource`).
- Requests from web pages are refused unless they carry the token: a request with an `Origin`
  header, or with a `Host` other than `127.0.0.1:<port>` / `localhost:<port>`, needs it. Commands
  must be sent as `application/json`.

## Reading

| Request | Answer |
|---|---|
| `GET /api/status` | `{"type":"status", "profile", "profile_path", "layer", "slots", "active_slot", "remap_on", "auto_switch", "foreground", "virtual_pad", "hidden_devices", "warnings"}` |
| `GET /api/devices` | `{"type":"devices", "devices":[{"id", "name", "backend", "vendor_id", "product_id", "instance_paths"}]}` |
| `GET /api/live` | Everything the app's window shows: devices with their state and battery, slots, layer, overlays, settings. Large; meant for tools that need it. It is the app's own state, so its `active_slot` counts from 0. |
| `GET /api/events` | A Server-Sent Events stream: an `event: live` with a short summary (`remap_on`, `suspended`, `active_slot`, `profile`, `layer`, `layer_name`, `devices` with `battery`, `warnings`) whenever it changes, and a comment line now and then. |

Slots are numbered 1–4 in the status, the events and the commands.

## Commands

`POST /api` with the same JSON requests the `doppelpad` command-line tool sends:

| Body | Does |
|---|---|
| `{"cmd":"remap","on":true}` | Turn remapping on (the active slot) or off |
| `{"cmd":"select_slot","slot":2}` | Make slot 2 active (1–4) and turn remapping on |
| `{"cmd":"apply_to_slot","path":"C:\\…\\config.toml","slot":1}` | Put a config into a slot and activate it |
| `{"cmd":"clear_slot","slot":3}` | Empty a slot (`"slot": null` empties all) |
| `{"cmd":"apply","path":"…"}` / `{"cmd":"stop"}` | Run a config outside the slots / stop it |
| `{"cmd":"auto_switch","enabled":false}` | Automatic switching by the game in front |
| `{"cmd":"reload"}` | Read the profiles folder again |

`select_slot`, `apply_to_slot` and `clear_slot` take `"device": "<id>"` for a controller with
slots of its own (ids from `/api/devices`). Answers are `{"type":"ok","message":…}` or, with
status 400, `{"type":"error","message":…}`. Quitting Doppelpad is not possible over HTTP.

## Examples

```sh
curl http://127.0.0.1:26761/api/status
curl -X POST -H "Content-Type: application/json" -d '{"cmd":"select_slot","slot":2}' http://127.0.0.1:26761/api
curl -N http://127.0.0.1:26761/api/events
```
