# Existing local Codex CDP attachment: live evidence (2026-07-26)

> Status: Research snapshot
> Owns: Dated live evidence for attaching CoPets to an already loopback-CDP-enabled official Codex App
> Update when: The installed Codex App, process/listener shape, or `Rf` probe result is reverified
> Last verified: 2026-07-26

## Scope

This is a sanitized, user-authorized local probe of the installed official App `26.721.41059` on
macOS. It did not patch the App, read conversation text, or send a real follow-up, steer, approval,
or stop. Runtime PID, port, target IDs, and user content are intentionally omitted.

## Method

1. Confirm the official App's main process was launched with a `--remote-debugging-port=<port>`
   argument and had the same effective UID as CoPets.
2. Query `lsof` for that port's TCP LISTEN holders. Require the official App main process to own an
   IPv4 `127.0.0.1:<port>` listener; observe that a child helper can inherit the same descriptor.
3. Query only `http://127.0.0.1:<port>/json/list`, retain `app://-/index.html` renderer targets, and
   execute the guarded `Rf` resolver over CDP.
4. Run the fixed no-content sentinel only: a non-existent conversation ID with an empty prompt and
   nullable service tier. No selected task identity or prompt was supplied.

## Observations

- The official App main process owned the loopback listener. A child helper also appeared in `lsof`
  because it inherited the descriptor, so listener ownership must be tied to the main executable and
  command rather than the first PID returned by `lsof`.
- Two `app://-/index.html`-family page targets were present (main window and avatar overlay).
- The module-preload resolver found the installed `app-initial-*.js` asset and an export whose exact
  source was `function Rf(e,t){return _Ze.sendRequest(e,t)}`.
- The sentinel rejected with the recognized missing-AppServerManager condition for the fixed
  non-existent ID. This is an accepted no-content fingerprint result and does not create a user turn.

## Product re-verification

After rebuilding the debug CoPets bundle, the user-authorized **Automatic → Connect existing** path
found the single eligible App and displayed `Ready` / `Connected to the existing Codex bridge`.
CoPets was restarted for the check; the existing Codex App stayed running with its same loopback
listener, and the bridge remained Ready across multiple external-liveness monitor intervals. No
Ready follow-up, Steer, approval, answer, or stop was sent.

This verifies the automatic half of product gate C0e for this local build. It does not reverify the
custom-port half of C0e or either real control gate C2/C2b.

## Conclusion

An existing App can be a safe-enough experimental Channel B candidate only after an explicit user
action and all of these checks: exact same-user official process, exact loopback listener, renderer
target restriction, exact `Rf` source, and recognized no-content rejection. This evidence does not
make arbitrary DevTools attachment safe, does not cover remote endpoints, and does not prove a real
follow-up through the CoPets UI; the custom C0e and C2/C2b gates remain required.

## References

- [CDP `Rf` handler live gate](codex-cdp-rf-handler-live-2026-07-26.md)
- [CDP follow-up channel](../architecture/cdp-follow-up-channel.md)
- [ADR 0006](../decisions/0006-explicit-existing-cdp-attach.md)
