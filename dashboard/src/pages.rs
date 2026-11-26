const SHELL: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover">
<meta name="color-scheme" content="light">
<meta name="theme-color" content="#eef1ec">
<meta name="description" content="Control panel for the local REST APIs.">
<title>local-restapi</title>
<link rel="icon" href="/favicon.svg" type="image/svg+xml">
<link rel="stylesheet" href="/assets/app.css">
</head>
<body>
<a class="skip" href="#main">Skip to content</a>
<div id="root"></div>
<noscript><p class="noscript">This dashboard needs JavaScript to run.</p></noscript>
<script src="/assets/react.js"></script>
<script src="/assets/react-dom.js"></script>
<script src="/assets/htm.js"></script>
<script src="/assets/app.js"></script>
</body>
</html>"##;

pub fn shell() -> String {
    SHELL.to_string()
}
