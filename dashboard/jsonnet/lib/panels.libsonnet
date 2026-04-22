// sarDa panel helpers.
// Provides factory functions for the SAR-CSV Grafana dashboard panels.
// All panels use the Infinity data-source to read semicolon-delimited CSV files.

local g = import '../g.libsonnet';
local ts = g.panel.timeSeries;
local row = g.panel.row;
local custom = ts.fieldConfig.defaults.custom;

{
  // ---------------------------------------------------------------------------
  // UQL query strings
  // ---------------------------------------------------------------------------
  uql: {
    // Pivoted CSV where the "sum" and "-1" aggregate columns must be dropped.
    // Used for: per-CPU and interrupts panels.
    cpu: |||
      parse-csv --delimiter ";"
      | project-away "sum","-1"
      | extend "timestamp"=todatetime("timestamp")
    |||,

    // Pivoted CSV without column removal.
    // Used for: disk, network-dev, tty, power panels.
    pivoted: |||
      parse-csv --delimiter ";"
      | extend "timestamp"=todatetime("timestamp")
    |||,

    // Plain CSV (no pivoting required).
    simple: 'parse-csv',
  },

  // ---------------------------------------------------------------------------
  // Infinity data-source target
  // ---------------------------------------------------------------------------
  csvTarget(url, uql): {
    columns: [
      {
        selector: 'timestamp',
        text: 'datetime',
        timestampFormat: '2006/01/02 15:04:05',
        type: 'timestamp',
      },
    ],
    csv_options: {
      comment: '!',
      delimiter: ';',
      skip_empty_lines: true,
      skip_lines_with_error: true,
    },
    filters: [],
    format: 'timeseries',
    global_query_id: '',
    parser: 'uql',
    refId: 'A',
    root_selector: '',
    source: 'url',
    type: 'csv',
    uql: uql,
    url: url,
    url_options: { data: '', method: 'GET' },
  },

  // ---------------------------------------------------------------------------
  // Timeseries panel – standard SAR-CSV variant
  //
  // Parameters:
  //   title      – panel title
  //   url        – CSV file URL (typically "$baseurl/<metric>.csv")
  //   uqlType    – one of 'cpu' | 'pivoted' | 'simple'
  //   showSeries – when set, only this series name is visible in the viz
  //                (all others are hidden via a field override)
  // ---------------------------------------------------------------------------
  csvPanel(title, url, uqlType='simple', showSeries=null):
    ts.new(title)
    + ts.queryOptions.withTargets([self.csvTarget(url, self.uql[uqlType])])
    + ts.queryOptions.withDatasource('yesoreyeram-infinity-datasource', '${SAR_CSV_DATASOURCE}')
    + ts.panelOptions.withTransparent(true)
    + ts.gridPos.withH(10)
    + ts.gridPos.withW(24)

    // Appearance defaults that differ from Grafana's out-of-the-box values
    + custom.withDrawStyle('points')
    + custom.withPointSize(3)
    + custom.withShowPoints('auto')
    + custom.withLineWidth(1)
    + custom.withFillOpacity(0)
    + custom.withGradientMode('none')
    + custom.withSpanNulls(false)
    + custom.withInsertNulls(false)
    + custom.withBarAlignment(0)
    + custom.withBarWidthFactor(0.6)
    + custom.withLineInterpolation('linear')
    + custom.hideFrom.withViz(false)
    + custom.hideFrom.withLegend(false)
    + custom.hideFrom.withTooltip(false)
    + custom.stacking.withMode('none')
    + custom.stacking.withGroup('A')
    + custom.scaleDistribution.withType('linear')
    + custom.thresholdsStyle.withMode('off')

    // Legend & tooltip
    + ts.options.legend.withShowLegend(false)
    + ts.options.legend.withDisplayMode('list')
    + ts.options.legend.withPlacement('bottom')
    + ts.options.tooltip.withMode('single')
    + ts.options.tooltip.withSort('none')
    + { options+: { tooltip+: { hideZeros: false } } }

    // Optional series visibility override: show only `showSeries`, hide others.
    // Uses Grafana's built-in "hideSeriesFrom" system override pattern.
    + (if showSeries != null then
        ts.standardOptions.withOverrides([
          {
            __systemRef: 'hideSeriesFrom',
            matcher: {
              id: 'byNames',
              options: {
                mode: 'exclude',
                names: [showSeries],
                prefix: 'All except:',
                readOnly: true,
              },
            },
            properties: [
              {
                id: 'custom.hideFrom',
                value: { legend: false, tooltip: false, viz: true },
              },
            ],
          },
        ])
      else {}),

  // ---------------------------------------------------------------------------
  // Row panel factory
  //
  // Parameters:
  //   title      – row title
  //   collapsed  – whether the row starts collapsed (default: true)
  //   innerPanels – array of panels to embed in the row when collapsed
  // ---------------------------------------------------------------------------
  rowPanel(title, collapsed=true, innerPanels=[]):
    row.new(title)
    + row.withCollapsed(collapsed)
    + (if std.length(innerPanels) > 0 then row.withPanels(innerPanels) else {}),
}
