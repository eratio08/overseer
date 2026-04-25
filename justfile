set shell := ["bash", "-euo", "pipefail", "-c"]

version := `node npm/scripts/local-package-meta.mjs version`
main_package_name := `node npm/scripts/local-package-meta.mjs mainPackageName`
platform_key := `node npm/scripts/local-package-meta.mjs platformKey`
rust_target := `node npm/scripts/local-package-meta.mjs rustTarget`
platform_package_name := `node npm/scripts/local-package-meta.mjs platformPackageName`
main_tarball := `node npm/scripts/local-package-meta.mjs mainTarball`
platform_tarball := `node npm/scripts/local-package-meta.mjs platformTarball`
npm_prefix := `node npm/scripts/local-package-meta.mjs npmPrefix`
installed_os := `node npm/scripts/local-package-meta.mjs installedOs`

help:
  @printf '%s\n' \
    'just platform-info' \
    'just build' \
    'just pack' \
    'just install-global' \
    'just reinstall-global' \
    'just smoke-global' \
    'just clean-pack' \
    'just uninstall-global'

platform-info:
  @printf '%s\n' \
    'version: {{version}}' \
    'platform: {{platform_key}}' \
    'rust target: {{rust_target}}' \
    'main package: {{main_package_name}}' \
    'platform package: {{platform_package_name}}' \
    'main tarball: npm/overseer/{{main_tarball}}' \
    'platform tarball: npm/overseer-{{platform_key}}/{{platform_tarball}}' \
    'npm prefix: {{npm_prefix}}' \
    'installed os: {{installed_os}}'

build-cli:
  cd overseer && cargo build --release --target {{rust_target}}

build-host:
  @[ -d host/node_modules ] || (cd host && npm ci)
  cd host && npm run build

build-ui:
  @[ -d ui/node_modules ] || (cd ui && npm ci)
  cd ui && npm run build

build: build-cli build-host build-ui
  @true

assemble-npm:
  node npm/scripts/build-npm-package.mjs

generate-platform:
  node npm/scripts/generate-platform-package.mjs {{platform_key}} {{version}}

pack: build assemble-npm generate-platform
  cp overseer/target/{{rust_target}}/release/os npm/overseer-{{platform_key}}/os
  chmod +x npm/overseer-{{platform_key}}/os
  cd npm/overseer-{{platform_key}} && npm pack
  cd npm/overseer && npm pack

install-global: pack
  npm install -g npm/overseer-{{platform_key}}/{{platform_tarball}}
  OVERSEER_SKIP_POSTINSTALL=1 npm install -g npm/overseer/{{main_tarball}}

reinstall-global:
  just uninstall-global
  just install-global

smoke-global:
  "{{installed_os}}" --version
  "{{installed_os}}" task --help
  "{{installed_os}}" mcp --help
  "{{installed_os}}" ui --help

clean-pack:
  rm -f npm/overseer/*.tgz npm/overseer-{{platform_key}}/*.tgz

uninstall-global:
  npm uninstall -g {{platform_package_name}} {{main_package_name}} @dmmulroy/overseer @dmmulroy/overseer-{{platform_key}} || true
