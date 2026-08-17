## Overview

`stampede` is a batch transcoding tool meant to be used for large media libraries. It performs transcoding jobs in batches and delegates threading options to the underling decoder/encoder used by individual transcoders in the application. To see all available config options please refer to the [config example](config.example.yml)

## Usage

It is recommended to run this as a cronjob with a `@daily` cadence. An entry in your crontab could look like the following, assuming `stampede` is in your path

```sh
@daily stampede transcode -c ~/.config/stampede_config.yml
```

run `export STAMPEDE_CONFIG=<path-to-config>` to not have to specify `-c` each time you call `stampede` 