<a name="readme-top"></a>

[![Contributors][contributors-shield]][contributors-url]
[![Forks][forks-shield]][forks-url]
[![Issues][issues-shield]][issues-url]
[![GPLv3 License][license-shield]][license-url]

<br />
<div align="center">
  <h3 align="center">ruuth</h3>

  <p align="center">
    A simple nginx auth_request backend providing MFA and lockout mechanisms
    <br />
    <a href="https://github.com/outurnate/ruuth/issues">Report Bug</a>
    ·
    <a href="https://github.com/outurnate/ruuth/issues">Request Feature</a>
  </p>
</div>

<details>
  <summary>Table of Contents</summary>
  <ol>
    <li>
      <a href="#about-the-project">About The Project</a>
    </li>
    <li>
      <a href="#getting-started">Getting Started</a>
      <ul>
        <li><a href="#installing-ruuth">Installing ruuth</a></li>
        <ul>
          <li><a href="#source-installation">Source Installation</a></li>
          <li><a href="#rpm-installation">RPM Installation</a></li>
        </ul>
        <li><a href="#configuring-your-webserver">Configuring Your Webserver</a></li>
        <ul>
          <li><a href="#nginx">NGINX</a></li>
        </ul>
      </ul>
    </li>
    <li><a href="#usage">Usage</a></li>
    <li><a href="#contributing">Contributing</a></li>
    <li><a href="#license">License</a></li>
    <li><a href="#contact">Contact</a></li>
  </ol>
</details>

## About The Project

A simple self-hosted authentication backend for nginx.  Has the following features:

* Multiple database backends (sqlite, mysql, postgres)
* Support for running in a cluster
* Faking logins and/or presenting a captcha after a given number of failed attempts from a given source
* Pure rust with `#![forbid(unsafe_code)]`
* Proper handling of credential material.  All passwords salted+peppered with argon2id

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Getting Started

Instructions are provided below to install from source or install via package manager.  Once ruuth is installed and configured, NGINX must be configured to use it as an authentication backend

### Installing ruuth

Distro packages are provided where possible.  For all other cases, project must be build/run from source

#### Source Installation

1. Clone the repo
   ```sh
   git clone https://github.com/outurnate/ruuth.git
   ```
2. Build via cargo
   ```sh
   cargo build --release
   ```
3. Customize the config file.  A template is provided at `pkg/ruuth.toml`
4. Start the server
   ```sh
   ./target/release/ruuth --config ./pkg/ruuth.toml run
   ```

#### RPM Installation

1. Download the RPM package
   ```sh
   curl -OsSL https://github.com/Outurnate/ruuth/releases/download/v0.1.5/ruuth-0.1.5-1.x86_64.rpm
   ```
2. Install the package
   ```sh
   sudo rpm -i ruuth-0.1.5-1.x86_64.rpm
   ```
3. Customize the config file
   ```sh
   sudo vi /etc/ruuth.toml
   ```
4. Enable and start the systemd service
   ```sh
   sudo systemctl enable --now ruuth
   ```

### Configuring Your Webserver

#### NGINX

Minimal sample configuration is provided below.  You will want to consult your distro's provided `nginx.conf` to ensure you do not overwrite any necessary customizations

    worker_processes 1;

    error_log  /var/log/nginx/error.log;
    error_log  /var/log/nginx/error.log notice;
    error_log  /var/log/nginx/error.log info;

    events { worker_connections 1024; }

    # We define a shared backend definition that refers
    # to the local/remote instance of ruuth.  Update this
    # to match your config file
    upstream ruuth
    {
      server 127.0.0.1:3000;
    }

    http
    {
      include            mime.types;
      default_type       application/octet-stream;
      sendfile           on;
      keepalive_timeout  65;

      # This server block represents the resource we wish
      # to secure.  We define an internal-only validate
      # route for NGINX to reach our auth server.  We can
      # define any number of these for different applications
      server
      {
        listen       443 ssl http2;
        server_name  example.com;

        ssl_certificate      cert.pem;
        ssl_certificate_key  key.pem;

        ssl_session_cache    shared:SSL:1m;
        ssl_session_timeout  5m;

        ssl_ciphers  HIGH:!aNULL:!MD5;
        ssl_prefer_server_ciphers  on;

        location /validate
        {
          # We wish to ensure we do not unnecessarily expose ruuth to the internet
          internal;
          # Pass the validate path to the backend
          proxy_pass              http://ruuth/validate;
          proxy_set_header        Host $http_host;
          # ruuth ignores the body, so we should discard it
          proxy_pass_request_body off;
          proxy_set_header        Content-Length "";
          # We include the original IP for rate-limiting and blacklisting
          proxy_set_header        X-Forwarded-For $proxy_add_x_forwarded_for;
          # Lastly, we need to pass through cookies to ensure the validation
          # handler is able to see the user's session cookie
          auth_request_set        $saved_set_cookie $upstream_http_set_cookie;
          proxy_set_header        Set-Cookie $saved_set_cookie;
        }

        # Setup auth_request and define the 401 handler
        auth_request /validate;
        error_page 401 @error401;
        location @error401
        {
          # In place of a 401 error, we rewrite to a 302 that shows the login page
          return 302 https://auth.example.com/?url=$scheme://$http_host$request_uri;
        }

        # Here's our dummy content
        location /
        {
          root  /usr/share/nginx/html;
          index index.html index.htm;
        }
      }

      # We define a single authentication server for serving
      # the login/logout pages and handlers
      server
      {
        listen 443 ssl http2;
        server_name auth.example.com;

        ssl_certificate      cert.pem;
        ssl_certificate_key  key.pem;

        ssl_session_cache    shared:SSL:1m;
        ssl_session_timeout  5m;

        ssl_ciphers  HIGH:!aNULL:!MD5;
        ssl_prefer_server_ciphers  on;

        location /
        {
          # Pass the validate path to the backend
          proxy_pass                       http://ruuth;
          proxy_set_header Host            $http_host;
          # We include the original IP for rate-limiting and blacklisting
          proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        }
      }
    }

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Usage

To add users to the database, use the following command

    ruuth --config /etc/ruuth.toml add-user --username hblue

You will be prompted to set a password.  Once the password is accepted, the TOTP secret URL will be shown.  To disable the QR code on the terminal, use the following form

    ruuth --config /etc/ruuth.toml add-user --username hblue --show-qr-code

Users can be deleted with the following command

    ruuth --config /etc/ruuth.toml delete-user --username hblue

To reset the password for a user, use the following command

    ruuth --config /etc/ruuth.toml reset-password --username hblue

To generate a new TOTP secret for a user, use the following command (also supports `--show-qr-code`)

    ruuth --config /etc/ruuth.toml reset-mfa --username hblue

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Contributing

Contributions are what make the open source community such an amazing place to learn, inspire, and create. Any contributions you make are **greatly appreciated**.

If you have a suggestion that would make this better, please fork the repo and create a pull request. You can also simply open an issue with the tag "enhancement"

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## License

Distributed under the GPLv3 License. See `LICENSE.md` for more information.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Contact

Your Name - [@outurnate](https://twitter.com/outurnate) - joseph@outurnate.com

Project Link: [https://github.com/outurnate/ruuth](https://github.com/outurnate/ruuth)

<p align="right">(<a href="#readme-top">back to top</a>)</p>

[contributors-shield]: https://img.shields.io/github/contributors/outurnate/ruuth.svg?style=for-the-badge
[contributors-url]: https://github.com/outurnate/ruuth/graphs/contributors
[forks-shield]: https://img.shields.io/github/forks/outurnate/ruuth.svg?style=for-the-badge
[forks-url]: https://github.com/outurnate/ruuth/network/members
[issues-shield]: https://img.shields.io/github/issues/outurnate/ruuth.svg?style=for-the-badge
[issues-url]: https://github.com/outurnate/ruuth/issues
[license-shield]: https://img.shields.io/github/license/outurnate/ruuth.svg?style=for-the-badge
[license-url]: https://github.com/outurnate/ruuth/blob/master/LICENSE.md