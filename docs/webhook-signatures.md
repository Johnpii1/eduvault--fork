# Creator Webhook Signatures

EduVault signs creator webhooks with `X-EduVault-Signature`.

The header uses the format `t=<unix timestamp>,v1=<hex hmac>`. The HMAC is `HMAC-SHA256` over:

```text
<timestamp>.<raw JSON request body>
```

Use your account's `webhookSigningSecret` to recompute the digest, compare it with a constant-time equality check, and reject requests whose timestamp is outside a short replay window such as five minutes.
