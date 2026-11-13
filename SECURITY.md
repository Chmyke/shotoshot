# Security

Report a vulnerability privately through GitHub: open the **Security** tab of this repository and
choose **Report a vulnerability**. Please do not open a public issue for a security problem.

Only the latest released version is supported.

For context when assessing a report: Shotoshot sends nothing over the network on its own. Two
outbound requests exist, each made only when you click its button: the Google Lens upload, and
Check for updates, a plain GET of `https://shotoshot.com/api/shotoshot/latest` that carries no
identifier and downloads nothing else.
