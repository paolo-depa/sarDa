//! The Metric catalog: every Activity sarDa asks `sadf` for, and everything
//! derived from it (parsing rules, Prometheus metric names, dashboard panels,
//! the documentation table). Add a new Activity here and nothing else.

pub const HOST_COLUMN: &str = "# hostname";
pub const INTERVAL_COLUMN: &str = "interval";
pub const TIMESTAMP_COLUMN: &str = "timestamp";
pub const METRIC_PREFIX: &str = "sarda";

/// The column that splits an Activity into independent entities.
#[derive(Debug)]
pub struct Dimension {
    /// Column header in sadf's output, e.g. `IFACE`.
    pub column: &'static str,
    /// Prometheus label carrying the entity, e.g. `iface`.
    pub label: &'static str,
}

/// A dashboard panel grouping several related metrics of one Activity.
#[derive(Debug)]
pub struct PanelSpec {
    pub title: &'static str,
    pub columns: &'static [&'static str],
}

#[derive(Debug)]
pub struct Activity {
    /// Stable identifier, also the CSV export file stem.
    pub label: &'static str,
    /// Options passed to `sadf` after `--`.
    pub sadf_opts: &'static str,
    pub dimension: Option<Dimension>,
    /// Whether `sarda export-csv` also writes one pivoted file per metric.
    pub csv_pivot: bool,
    /// Dashboard row title.
    pub group: &'static str,
    pub description: &'static str,
    /// Explicit panels. Empty means: one panel per metric for dimensional
    /// Activities, a single panel with every metric otherwise.
    pub panels: &'static [PanelSpec],
}

impl Dimension {
    /// The label value for an entity as `sadf` prints it: trimmed, with CPU
    /// `-1` (all CPUs together) called `all`. The CSV export keeps the raw
    /// text instead, as the legacy files did.
    pub fn label_value(&self, raw: &str) -> String {
        let raw = raw.trim();
        if self.column == "CPU" && raw == "-1" {
            "all".to_string()
        } else {
            raw.to_string()
        }
    }
}

const fn dim(column: &'static str, label: &'static str) -> Option<Dimension> {
    Some(Dimension { column, label })
}

const fn panel(title: &'static str, columns: &'static [&'static str]) -> PanelSpec {
    PanelSpec { title, columns }
}

pub static ACTIVITIES: &[Activity] = &[
    Activity {
        label: "io",
        sadf_opts: "-b",
        dimension: None,
        csv_pivot: false,
        group: "IO",
        description: "Overall I/O transfer rates",
        panels: &[
            panel(
                "I/O transactions per second",
                &["tps", "rtps", "wtps", "dtps"],
            ),
            panel("I/O data rate", &["bread/s", "bwrtn/s", "bdscd/s"]),
        ],
    },
    Activity {
        label: "paging",
        sadf_opts: "-B",
        dimension: None,
        csv_pivot: false,
        group: "Paging",
        description: "Paging statistics",
        panels: &[
            panel(
                "Memory management",
                &["pgpgin/s", "pgpgout/s", "pgfree/s", "fault/s", "majflt/s"],
            ),
            panel("Reclamation", &["pgscank/s", "pgscand/s", "pgsteal/s"]),
            panel("NUMA management", &["pgprom/s", "pgdem/s"]),
        ],
    },
    Activity {
        label: "power_cpu",
        sadf_opts: "-m CPU",
        dimension: dim("CPU", "cpu"),
        csv_pivot: false,
        group: "Power",
        description: "CPU instantaneous clock frequency",
        panels: &[],
    },
    Activity {
        label: "power_fan",
        sadf_opts: "-m FAN",
        dimension: dim("FAN", "fan"),
        csv_pivot: false,
        group: "Power",
        description: "Fan speed",
        panels: &[],
    },
    Activity {
        label: "power_freq",
        sadf_opts: "-m FREQ",
        dimension: dim("CPU", "cpu"),
        csv_pivot: false,
        group: "Power",
        description: "CPU average clock frequency",
        panels: &[],
    },
    Activity {
        label: "power_in",
        sadf_opts: "-m IN",
        dimension: dim("IN", "input"),
        csv_pivot: false,
        group: "Power",
        description: "Voltage inputs",
        panels: &[],
    },
    Activity {
        label: "power_temp",
        sadf_opts: "-m TEMP",
        dimension: dim("TEMP", "sensor"),
        csv_pivot: false,
        group: "Power",
        description: "Device temperatures",
        panels: &[],
    },
    Activity {
        label: "power_USB",
        sadf_opts: "-m USB",
        dimension: dim("BUS", "bus"),
        csv_pivot: false,
        group: "Power",
        description: "USB devices power",
        panels: &[],
    },
    Activity {
        label: "disk",
        sadf_opts: "-d",
        dimension: dim("DEV", "dev"),
        csv_pivot: true,
        group: "Disks",
        description: "Block device activity",
        panels: &[],
    },
    Activity {
        label: "filesystem",
        sadf_opts: "-F",
        dimension: dim("FILESYSTEM", "filesystem"),
        csv_pivot: false,
        group: "Filesystems",
        description: "Mounted filesystem usage",
        panels: &[],
    },
    Activity {
        label: "hugepages",
        sadf_opts: "-H",
        dimension: None,
        csv_pivot: false,
        group: "Hugepages",
        description: "Hugepages utilization",
        panels: &[
            panel(
                "Hugepages",
                &["kbhugfree", "kbhugused", "kbhugrsvd", "kbhugsurp"],
            ),
            panel("Hugepages used", &["%hugused"]),
        ],
    },
    Activity {
        label: "interrupts",
        sadf_opts: "-I ALL",
        dimension: dim("INTR", "intr"),
        csv_pivot: true,
        group: "Interrupts",
        description: "Interrupts per second, per interrupt source",
        panels: &[],
    },
    Activity {
        label: "network_dev",
        sadf_opts: "-n DEV",
        dimension: dim("IFACE", "iface"),
        csv_pivot: true,
        group: "Network devices",
        description: "Network interface traffic",
        panels: &[],
    },
    Activity {
        label: "network_edev",
        sadf_opts: "-n EDEV",
        dimension: dim("IFACE", "iface"),
        csv_pivot: true,
        group: "Network devices",
        description: "Network interface errors",
        panels: &[],
    },
    Activity {
        label: "network_fc",
        sadf_opts: "-n FC",
        dimension: dim("FCHOST", "fchost"),
        csv_pivot: false,
        group: "Network FC",
        description: "Fibre Channel traffic",
        panels: &[],
    },
    Activity {
        label: "network_icmp",
        sadf_opts: "-n ICMP",
        dimension: None,
        csv_pivot: false,
        group: "Network ICMP",
        description: "IPv4 ICMP traffic",
        panels: &[],
    },
    Activity {
        label: "network_eicmp",
        sadf_opts: "-n EICMP",
        dimension: None,
        csv_pivot: false,
        group: "Network ICMP",
        description: "IPv4 ICMP errors",
        panels: &[],
    },
    Activity {
        label: "network_icmp6",
        sadf_opts: "-n ICMP6",
        dimension: None,
        csv_pivot: false,
        group: "Network ICMP",
        description: "IPv6 ICMP traffic",
        panels: &[],
    },
    Activity {
        label: "network_eicmp6",
        sadf_opts: "-n EICMP6",
        dimension: None,
        csv_pivot: false,
        group: "Network ICMP",
        description: "IPv6 ICMP errors",
        panels: &[],
    },
    Activity {
        label: "network_ip",
        sadf_opts: "-n IP",
        dimension: None,
        csv_pivot: false,
        group: "Network IP",
        description: "IPv4 traffic",
        panels: &[],
    },
    Activity {
        label: "network_eip",
        sadf_opts: "-n EIP",
        dimension: None,
        csv_pivot: false,
        group: "Network IP",
        description: "IPv4 errors",
        panels: &[],
    },
    Activity {
        label: "network_ip6",
        sadf_opts: "-n IP6",
        dimension: None,
        csv_pivot: false,
        group: "Network IP",
        description: "IPv6 traffic",
        panels: &[],
    },
    Activity {
        label: "network_eip6",
        sadf_opts: "-n EIP6",
        dimension: None,
        csv_pivot: false,
        group: "Network IP",
        description: "IPv6 errors",
        panels: &[],
    },
    Activity {
        label: "network_nfs",
        sadf_opts: "-n NFS",
        dimension: None,
        csv_pivot: false,
        group: "Network NFS",
        description: "NFS client activity",
        panels: &[],
    },
    Activity {
        label: "network_nfsd",
        sadf_opts: "-n NFSD",
        dimension: None,
        csv_pivot: false,
        group: "Network NFS",
        description: "NFS server activity",
        panels: &[],
    },
    Activity {
        label: "network_sock",
        sadf_opts: "-n SOCK",
        dimension: None,
        csv_pivot: false,
        group: "Network TCP/UDP",
        description: "IPv4 socket usage",
        panels: &[],
    },
    Activity {
        label: "network_sock6",
        sadf_opts: "-n SOCK6",
        dimension: None,
        csv_pivot: false,
        group: "Network TCP/UDP",
        description: "IPv6 socket usage",
        panels: &[],
    },
    Activity {
        label: "network_soft",
        sadf_opts: "-n SOFT",
        dimension: dim("CPU", "cpu"),
        csv_pivot: false,
        group: "Network software processing",
        description: "Software-based network processing, per CPU",
        panels: &[],
    },
    Activity {
        label: "network_tcp",
        sadf_opts: "-n TCP",
        dimension: None,
        csv_pivot: false,
        group: "Network TCP/UDP",
        description: "TCP traffic",
        panels: &[],
    },
    Activity {
        label: "network_etcp",
        sadf_opts: "-n ETCP",
        dimension: None,
        csv_pivot: false,
        group: "Network TCP/UDP",
        description: "TCP errors",
        panels: &[],
    },
    Activity {
        label: "network_udp",
        sadf_opts: "-n UDP",
        dimension: None,
        csv_pivot: false,
        group: "Network TCP/UDP",
        description: "IPv4 UDP traffic",
        panels: &[],
    },
    Activity {
        label: "network_udp6",
        sadf_opts: "-n UDP6",
        dimension: None,
        csv_pivot: false,
        group: "Network TCP/UDP",
        description: "IPv6 UDP traffic",
        panels: &[],
    },
    Activity {
        label: "per_cpu",
        sadf_opts: "-P ALL",
        dimension: dim("CPU", "cpu"),
        csv_pivot: true,
        group: "CPU",
        description: "CPU utilization, per CPU",
        panels: &[],
    },
    Activity {
        label: "queue",
        sadf_opts: "-q",
        dimension: None,
        csv_pivot: false,
        group: "Queue and load",
        description: "Run queue length and load average",
        panels: &[
            panel("Queue", &["runq-sz", "plist-sz", "blocked"]),
            panel("Load average", &["ldavg-1", "ldavg-5", "ldavg-15"]),
        ],
    },
    Activity {
        label: "memory",
        sadf_opts: "-r ALL",
        dimension: None,
        csv_pivot: false,
        group: "Memory",
        description: "Memory utilization",
        panels: &[
            panel(
                "General",
                &["kbmemfree", "kbavail", "kbmemused", "kbvmused"],
            ),
            panel("Active / inactive", &["kbactive", "kbinact"]),
            panel(
                "Kernel memory",
                &["kbbuffers", "kbslab", "kbkstack", "kbpgtbl"],
            ),
            panel(
                "Cached / committed / anonymous / dirty / shared",
                &["kbcached", "kbcommit", "kbdirty", "kbanonpg", "kbshmem"],
            ),
            panel("Percentages", &["%memused", "%commit"]),
        ],
    },
    Activity {
        label: "swap_util",
        sadf_opts: "-S",
        dimension: None,
        csv_pivot: false,
        group: "Swap",
        description: "Swap space utilization",
        panels: &[
            panel("Swap space", &["kbswpfree", "kbswpused", "kbswpcad"]),
            panel("Swap percentages", &["%swpused", "%swpcad"]),
        ],
    },
    Activity {
        label: "inode",
        sadf_opts: "-v",
        dimension: None,
        csv_pivot: false,
        group: "Inodes",
        description: "Inode, file and pseudo-terminal tables",
        panels: &[],
    },
    Activity {
        label: "swap",
        sadf_opts: "-W",
        dimension: None,
        csv_pivot: false,
        group: "Swap",
        description: "Swap paging",
        panels: &[panel("Swap I/O", &["pswpin/s", "pswpout/s"])],
    },
    Activity {
        label: "task",
        sadf_opts: "-w",
        dimension: None,
        csv_pivot: false,
        group: "Task",
        description: "Context switches and process creation",
        panels: &[
            panel("Context switches", &["cswch/s"]),
            panel("Process creation", &["proc/s"]),
        ],
    },
    Activity {
        label: "tty",
        sadf_opts: "-y",
        dimension: dim("TTY", "tty"),
        csv_pivot: true,
        group: "TTY",
        description: "Serial line activity",
        panels: &[],
    },
];

#[cfg(test)]
pub fn activity(label: &str) -> Option<&'static Activity> {
    ACTIVITIES.iter().find(|a| a.label == label)
}

impl Activity {
    pub(crate) fn is_meta_column(&self, column: &str) -> bool {
        column == HOST_COLUMN
            || column == INTERVAL_COLUMN
            || column == TIMESTAMP_COLUMN
            || self.dimension.as_ref().is_some_and(|d| d.column == column)
    }
}

/// Prometheus-safe fragment for a sadf column header: `%util` -> `pct_util`,
/// `rkB/s` -> `rkB_s`, `CPU*` -> `intr_s`.
pub fn sanitize_column(column: &str) -> String {
    if column == "CPU*" {
        return "intr_s".into();
    }
    let mut out = String::new();
    for c in column.chars() {
        match c {
            '%' => out.push_str("pct_"),
            c if c.is_ascii_alphanumeric() || c == '_' => out.push(c),
            _ => out.push('_'),
        }
    }
    out
}

pub fn metric_name(activity_label: &str, column: &str) -> String {
    format!(
        "{METRIC_PREFIX}_{activity_label}_{}",
        sanitize_column(column)
    )
}

/// Grafana unit id for a sadf column, guessed from its header.
pub fn unit_for(column: &str) -> &'static str {
    if column.starts_with('%') {
        "percent"
    } else if column.contains("kB/s") {
        "KBs"
    } else if column.starts_with("kb") {
        "kbytes"
    } else if column.starts_with("MB") {
        "mbytes"
    } else if matches!(column, "await" | "svctm") {
        "ms"
    } else if column.contains("pck/s") {
        "pps"
    } else if column.ends_with("/s") {
        "ops"
    } else if column == "MHz" {
        "rotmhz"
    } else {
        "short"
    }
}

/// Markdown table of every Activity, embedded in the README and checked by a
/// test so the documentation cannot drift from the code.
pub fn markdown_table() -> String {
    let mut out = String::from(
        "| Dashboard row | Activity | `sadf` options | Dimension label | Metric prefix |\n|---|---|---|---|---|\n",
    );
    for a in ACTIVITIES {
        let dim = a
            .dimension
            .as_ref()
            .map_or("-".to_string(), |d| format!("`{}`", d.label));
        out.push_str(&format!(
            "| {} | {} | `{}` | {} | `{}_{}_` |\n",
            a.group, a.description, a.sadf_opts, dim, METRIC_PREFIX, a.label
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_labels_are_trimmed_and_all_cpus_are_called_all() {
        let cpu = activity("per_cpu").unwrap().dimension.as_ref().unwrap();
        assert_eq!(cpu.label_value("-1"), "all");
        assert_eq!(cpu.label_value(" 3 "), "3");
        let iface = activity("network_dev").unwrap().dimension.as_ref().unwrap();
        assert_eq!(iface.label_value("-1"), "-1", "only CPUs have an aggregate");
        assert_eq!(iface.label_value("eth0"), "eth0");
    }

    #[test]
    fn labels_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for a in ACTIVITIES {
            assert!(seen.insert(a.label), "duplicate label {}", a.label);
        }
    }

    #[test]
    fn descriptions_are_unique_so_panel_titles_are() {
        let mut seen = std::collections::HashSet::new();
        for a in ACTIVITIES {
            assert!(
                seen.insert(a.description),
                "duplicate description {}",
                a.description
            );
        }
    }

    #[test]
    fn metric_names_are_valid_prometheus_names() {
        let re = regex::Regex::new(r"^[a-zA-Z_:][a-zA-Z0-9_:]*$").unwrap();
        for col in [
            "%util",
            "rkB/s",
            "areq-sz",
            "CPU*",
            "kbmemfree",
            "ldavg-1",
            "%=memused",
        ] {
            let name = metric_name("x", col);
            assert!(re.is_match(&name), "{name}");
        }
    }

    #[test]
    fn sanitizing_examples() {
        assert_eq!(metric_name("disk", "%util"), "sarda_disk_pct_util");
        assert_eq!(
            metric_name("network_dev", "rxkB/s"),
            "sarda_network_dev_rxkB_s"
        );
        assert_eq!(metric_name("interrupts", "CPU*"), "sarda_interrupts_intr_s");
    }

    #[test]
    fn units() {
        assert_eq!(unit_for("%idle"), "percent");
        assert_eq!(unit_for("rxkB/s"), "KBs");
        assert_eq!(unit_for("kbmemfree"), "kbytes");
        assert_eq!(unit_for("await"), "ms");
        assert_eq!(unit_for("rxpck/s"), "pps");
        assert_eq!(unit_for("tps"), "short");
    }
}
