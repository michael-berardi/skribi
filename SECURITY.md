# Security policy

## Supported versions

Security fixes are provided for the latest published Skribi release.

## Report a vulnerability

Do not open a public issue for an undisclosed vulnerability. Use GitHub’s private vulnerability reporting for this repository. Include the affected version, operating system, reproduction steps, impact, and any proof-of-concept files needed to reproduce the issue.

You should receive an acknowledgment within five business days. Please allow time for a fix and coordinated disclosure before publishing details.

## Security boundaries

Skribi is a local desktop application. It does not provide access control, encryption, cloud backup, or malware scanning. A `.skribi` file or imported ZIP remains untrusted input: Skribi rejects path traversal, symlinks, excessive entry counts, oversized entries, and oversized expanded archives, but users should still obtain content from sources they trust.

Markdown links may open external applications or websites through the operating system. Skribi does not execute imported documents, scripts, or attachments.
