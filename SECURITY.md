# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in Arkived, please **do not file a public issue**. Instead, email details to:

**`hamza.abdagic@horizon-tech.io`**

Please include:

- A description of the vulnerability
- Steps to reproduce
- Impact assessment (what an attacker could do)
- Any suggested remediation

We will acknowledge your report within 72 hours and aim to provide an initial assessment within 7 days.

## Supported Versions

Arkived is pre-release (0.0.x). Only the latest published version receives security fixes.

Once we reach 1.0.0, we will publish a formal support matrix here.

## Scope

In scope:

- All code in this repository
- Published crates on crates.io under the `arkived-*` namespace
- Binaries distributed from GitHub Releases and `arkived.app`

Out of scope:

- The Microsoft Azure services Arkived connects to (report those to Microsoft)
- Third-party MCP clients or ACP agents
- Dependencies — report to the upstream project, though we appreciate being CC'd

## Disclosure policy

We follow coordinated disclosure. We will work with you to understand the issue, develop a fix, and coordinate a public disclosure date.
