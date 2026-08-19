`stampede` is a batch processing tool meant to be used for large media libraries. It performs transcoding/deadroll detection jobs in batches and delegates threading options to the underling decoder/encoder used by individual transcoders in the application. To see all available config options please refer to the [config example](config.example.yml)

This project is currently a heavy work in progress and it is not recommended to use it to any serious extent

## Usage

It is recommended to run this as a cronjob with a `@daily` cadence. An entry in your crontab could look like the following, assuming `stampede` is in your path

```sh
@daily stampede transcode -c ~/.config/stampede_config.yml
@daily stampede deadroll -c ~/.config/stampede_config.yml
```

run `export STAMPEDE_CONFIG=<path-to-config>` to not have to specify `-c` each time you call `stampede` 
