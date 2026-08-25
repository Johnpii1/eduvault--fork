# Security Policy

## Reporting a Vulnerability
If you discover a potential security vulnerability, please **do not open a public issue**. Instead, report it privately via email to ensure the safety of our users.

**Email:** security@eduvault.io (or your-email@example.com)

## Security Model
EduVault is a non-custodial platform:
- **Private Keys:** We never store or transmit private keys. Wallet interactions happen client-side via Reown/AppKit.
- **Transactions:** Users must manually approve all on-chain actions.
- **Infrastructure:** Sensitive keys are managed through secure environment variables.

## Rate-Limiting Trust Model
API rate limits are enforced by `withApiHardening` (src/lib/api/hardening.js). Client identification uses the following trust hierarchy:

| Header | Trusted? | Notes |
| :--- | :--- | :--- |
| `x-vercel-forwarded-for` | Always | Injected by Vercel's edge network; not overwritable by clients. |
| `x-real-ip` | Only with `TRUSTED_PROXY_COUNT` env | Trusted only when a known reverse proxy strips client forwarding headers. |
| `x-forwarded-for` | **Never** | Client-controllable; never used for rate-limit bucketing. |
| `x-real-ip` (no proxy) | **Never** | Falls back to metadata-based composite key. |

When no trusted IP header is available, the rate limiter derives a per-caller bucket from semi-stable request metadata (user-agent + accept-language), preventing a single shared "local" bucket while still resisting header-spoofing resets.

**Environment variables:**
- `TRUSTED_PROXY_COUNT` — Set to a positive integer when deploying behind a reverse proxy that overwrites `x-real-ip`. Only set this when you control the proxy layer.

## Scope
| Component | Status |
| :--- | :--- |
| EduVault Frontend | In Scope |
| EduVault API Routes | In Scope |
| Smart Contracts | In Scope |
| 3rd Party Services (Clerk, MongoDB) | Out of Scope |

## Disclosure Policy
We commit to acknowledging all reports within 48 hours and will work to resolve valid issues as quickly as possible.