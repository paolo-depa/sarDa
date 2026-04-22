import re
from typing import Any, Dict

SADF_TIMEOUT_RC = -1
SADF_ERROR_RC = -2
TIMESTAMP_COL = "timestamp"
TIMESTAMP_FORMAT = "%Y%m%dT%H%M%S"

FILTER_REGEXPS = [re.compile(r"RESTART"), re.compile(r"^#")]
FORMAT_CONFIG = {"format": "csv", "sadf_arg": "-d", "separator": ";"}

AGGREGATORS: Dict[str, Dict[str, Any]] = {
    "io": {"sar_param": "-b"},
    "paging": {"sar_param": "-B"},
    "power_cpu": {"sar_param": "-m CPU"},
    "power_fan": {"sar_param": "-m FAN"},
    "power_freq": {"sar_param": "-m FREQ"},
    "power_in": {"sar_param": "-m IN"},
    "power_temp": {"sar_param": "-m TEMP"},
    "power_USB": {"sar_param": "-m USB"},
    "disk": {
        "sar_param": "-d",
        "pivot": {
            "index": [TIMESTAMP_COL],
            "columns": ["DEV"],
            "skip_columns": ["# hostname", "interval"],
        },
    },
    "filesystem": {"sar_param": "-F"},
    "hugepages": {"sar_param": "-H"},
    "interrupts": {
        "sar_param": "-I ALL",
        "pivot": {
            "index": [TIMESTAMP_COL],
            "columns": ["INTR"],
            "skip_columns": ["# hostname", "interval"],
        },
    },
    "network_dev": {
        "sar_param": "-n DEV",
        "pivot": {
            "index": [TIMESTAMP_COL],
            "columns": ["IFACE"],
            "skip_columns": ["# hostname", "interval"],
        },
    },
    "network_edev": {
        "sar_param": "-n EDEV",
        "pivot": {
            "index": [TIMESTAMP_COL],
            "columns": ["IFACE"],
            "skip_columns": ["# hostname", "interval"],
        },
    },
    "network_fc": {"sar_param": "-n FC"},
    "network_icmp": {"sar_param": "-n ICMP"},
    "network_eicmp": {"sar_param": "-n EICMP"},
    "network_icmp6": {"sar_param": "-n ICMP6"},
    "network_eicmp6": {"sar_param": "-n EICMP6"},
    "network_ip": {"sar_param": "-n IP"},
    "network_eip": {"sar_param": "-n EIP"},
    "network_ip6": {"sar_param": "-n IP6"},
    "network_eip6": {"sar_param": "-n EIP6"},
    "network_nfs": {"sar_param": "-n NFS"},
    "network_nfsd": {"sar_param": "-n NFSD"},
    "network_sock": {"sar_param": "-n SOCK"},
    "network_sock6": {"sar_param": "-n SOCK6"},
    "network_soft": {"sar_param": "-n SOFT"},
    "network_tcp": {"sar_param": "-n TCP"},
    "network_etcp": {"sar_param": "-n ETCP"},
    "network_udp": {"sar_param": "-n UDP"},
    "network_udp6": {"sar_param": "-n UDP6"},
    "per_cpu": {
        "sar_param": "-P ALL",
        "pivot": {
            "index": [TIMESTAMP_COL],
            "columns": ["CPU"],
            "skip_columns": ["# hostname", "interval"],
        },
    },
    "queue": {"sar_param": "-q"},
    "memory": {"sar_param": "-r ALL"},
    "swap_util": {"sar_param": "-S"},
    "inode": {"sar_param": "-v"},
    "swap": {"sar_param": "-W"},
    "task": {"sar_param": "-w"},
    "tty": {
        "sar_param": "-y",
        "pivot": {
            "index": [TIMESTAMP_COL],
            "columns": ["TTY"],
            "skip_columns": ["# hostname", "interval"],
        },
    },
}
