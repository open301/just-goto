# Security Policy

## Supported Versions

Security fixes are provided for the latest released version of JustGoto.

| Version        | Supported |
| -------------- | --------- |
| Latest         | ✅        |
| Older versions | ❌        |

Users are encouraged to keep their JustGoto installation up to date.

## Reporting a Vulnerability

Please do not report security vulnerabilities through public GitHub issues, discussions, or pull requests.

### Using GitHub

If you believe you have found a security vulnerability in JustGoto, please report it privately using GitHub's **Private Vulnerability Reporting** feature:

1. Open the repository on GitHub.
2. Go to **Security**.
3. Select **Advisories**.
4. Select **Report a vulnerability**.

### Other options

- Twitter (@zephnx_)
- Bluesky (@zephyrbash.dev)
- Email options are not available at the moment

## What to include in your report

Please include as much of the following information as possible:

- A description of the vulnerability.
- The affected version or commit.
- Steps required to reproduce the issue.
- The expected and actual behaviour.
- The potential security impact.
- Any proof-of-concept code or requests, if applicable.
- Suggested mitigations or fixes, if you have any.

Reports containing sufficient reproduction information are much easier to investigate.

## Security Scope

Examples of security issues that are particularly relevant to JustGoto include:

- Authentication or authorization bypasses.
- Unauthorized creation, modification, or deletion of redirects.
- Access to administrative functionality without the required permissions.
- Session fixation, session hijacking, or insecure session handling.
- SQL injection or other database-related vulnerabilities.
- Cross-site scripting (XSS).
- Cross-site request forgery (CSRF).
- Server-side request forgery (SSRF).
- Open redirects that bypass configured redirect restrictions.
- Redirect validation bypasses.
- Privilege escalation between user roles.
- Exposure of passwords, session tokens, secrets, or other sensitive information.
- Weaknesses that allow an attacker to redirect trusted links to malicious destinations.
- Denial-of-service vulnerabilities that can be triggered remotely with unreasonable resource usage.

## Out of Scope

The following are generally not considered security vulnerabilities:

- Vulnerabilities that require the attacker to already have full administrator access.
- Issues caused solely by an insecure deployment or reverse proxy configuration.
- Missing security headers without a demonstrated security impact.
- Rate-limit concerns without a practical denial-of-service or abuse scenario.
- Vulnerabilities in unsupported or significantly outdated versions of JustGoto.
- Social engineering attacks against JustGoto administrators.
- Reports generated entirely by automated scanners without evidence of an exploitable issue.

Dependencies are updated weekly, and we have security alerts turned on for them.

## Redirect Abuse

JustGoto is designed to allow authorised users to create and manage URL redirects.

The ability of an authorised administrator to create a redirect to an arbitrary destination is therefore not, by itself, considered a vulnerability.

However, please report any issue that allows:

- An unauthorised user to create or change redirects.
- Redirect destination restrictions to be bypassed.
- A redirect to be modified without the required permissions.
- Trusted JustGoto URLs to be redirected to attacker-controlled destinations without administrative access.

## Self-Hosted Deployments

JustGoto is intended to be self-hosted.

Operators are responsible for securely configuring their deployment, including:

- TLS/HTTPS.
- Reverse proxies.
- Firewalls.
- Environment variables and secrets.
- Operating system security.
- Backups.
- Access to the administrative interface.

A vulnerability in JustGoto itself should still be reported even if exploitation depends on a commonly used deployment configuration.

## Disclosure

Please allow reasonable time for a vulnerability to be investigated and fixed before publicly disclosing it.

Where appropriate, confirmed vulnerabilities may result in:

- A security advisory.
- A patched release.
- A CVE request.
- Credit to the reporter, unless anonymity is requested.

## Security Updates

Security-related releases will be published through the repository's GitHub Releases and, where appropriate, GitHub Security Advisories.
