// SAR-CSV Grafana dashboard – generated via jsonnet + grafonnet.
// Edit this file (and lib/panels.libsonnet) to maintain the dashboard;
// then run `make build-dashboard` to produce dashboard/sar-csv.json.

local g = import 'g.libsonnet';
local p = import 'lib/panels.libsonnet';

local dash = g.dashboard;
local row  = g.panel.row;

// ---------------------------------------------------------------------------
// Dashboard-level constants
// ---------------------------------------------------------------------------
local dashboardUid  = 'd5763753-d3a2-460b-8d9d-a9d534ae9d9b';
local dashboardTitle = 'SAR-CSV';

// ---------------------------------------------------------------------------
// Template variables
// ---------------------------------------------------------------------------
local variables = [
  // baseurl: hidden constant – set to the directory that serves your CSV files.
  {
    hide: 2,
    name: 'baseurl',
    query: '<!-- YOUR URL HERE -->',
    skipUrlSync: true,
    type: 'constant',
  },
  // SAR_CSV_DATASOURCE: the Infinity data-source used by all panels.
  {
    allowCustomValue: false,
    current: { text: 'SAR CSV Data Source', value: 'aerbn4xypvv28a' },
    name: 'SAR_CSV_DATASOURCE',
    options: [],
    query: 'yesoreyeram-infinity-datasource',
    refresh: 1,
    regex: '/^SAR.*/',
    type: 'datasource',
  },
];

// ---------------------------------------------------------------------------
// Panel definitions – grouped by section / row
// ---------------------------------------------------------------------------

// --- CPU ---
local cpuPanels = [
  p.csvPanel('idle',   '$baseurl/per_cpu__idle.csv',   'cpu'),
  p.csvPanel('iowait', '$baseurl/per_cpu__iowait.csv', 'cpu'),
  p.csvPanel('system', '$baseurl/per_cpu__system.csv', 'cpu'),
  p.csvPanel('user',   '$baseurl/per_cpu__user.csv',   'cpu'),
  p.csvPanel('irq',    '$baseurl/per_cpu__irq.csv',    'cpu'),
  p.csvPanel('soft',   '$baseurl/per_cpu__soft.csv',   'cpu'),
  p.csvPanel('nice',   '$baseurl/per_cpu__nice.csv',   'cpu'),
  p.csvPanel('gnice',  '$baseurl/per_cpu__gnice.csv',  'cpu'),
  p.csvPanel('steal',  '$baseurl/per_cpu__steal.csv',  'cpu'),
  p.csvPanel('guest',  '$baseurl/per_cpu__guest.csv',  'cpu'),
];

// --- Disks ---
local diskPanels = [
  p.csvPanel('%util',   '$baseurl/disk__util.csv',  'pivoted'),
  p.csvPanel('await',   '$baseurl/disk_await.csv',  'pivoted'),
  p.csvPanel('svctm',   '$baseurl/disk_svctm.csv',  'pivoted'),
  p.csvPanel('tps',     '$baseurl/disk_tps.csv',    'pivoted'),
  p.csvPanel('wkB_s',   '$baseurl/disk_wkB_s.csv',  'pivoted'),
  p.csvPanel('rkB_s',   '$baseurl/disk_rkB_s.csv',  'pivoted'),
  p.csvPanel('areq-sz', '$baseurl/disk_areq-sz.csv','pivoted'),
  p.csvPanel('aqu-sz',  '$baseurl/disk_aqu-sz.csv', 'pivoted'),
];

// --- Hugepages ---
local hugepagesPanels = [
  p.csvPanel('hugepages', '$baseurl/hugepages.csv'),
];

// --- Inodes ---
local inodePanels = [
  p.csvPanel('inode', '$baseurl/inode.csv'),
];

// --- I/O ---
local ioPanels = [
  p.csvPanel('TPS',       '$baseurl/io.csv'),
  p.csvPanel('r/w per sec', '$baseurl/io.csv'),
];

// --- Interrupts ---
local interruptsPanels = [
  p.csvPanel('intr/s', '$baseurl/interrupts_intr_s.csv', 'cpu'),
];

// --- Memory ---
local memoryPanels = [
  p.csvPanel('General',                   '$baseurl/memory.csv', showSeries='kbmemfree'),
  p.csvPanel('Active/Inactive',           '$baseurl/memory.csv', showSeries='kbinact'),
  p.csvPanel('Kernel mem',                '$baseurl/memory.csv'),
  p.csvPanel('Cached/Committed/Anon/Dirty', '$baseurl/memory.csv', showSeries='kbcached'),
  p.csvPanel('memory perc.',              '$baseurl/memory.csv'),
];

// --- Network – device stats ---
local networkDevPanels = [
  p.csvPanel('rxkB_s',   '$baseurl/network_dev_rxkB_s.csv',  'pivoted'),
  p.csvPanel('txkB_s',   '$baseurl/network_dev_txkB_s.csv',  'pivoted'),
  p.csvPanel('rxpck_s',  '$baseurl/network_dev_rxpck_s.csv', 'pivoted'),
  p.csvPanel('txpck_s',  '$baseurl/network_dev_txpck_s.csv', 'pivoted'),
  p.csvPanel('rxcmp_s',  '$baseurl/network_dev_rxcmp_s.csv', 'pivoted'),
  p.csvPanel('txcmp_s',  '$baseurl/network_dev_txcmp_s.csv', 'pivoted'),
  p.csvPanel('rxmcst_s', '$baseurl/network_dev_rxmcst_s.csv','pivoted'),
  p.csvPanel('%ifutil',  '$baseurl/network_dev__ifutil.csv', 'pivoted'),
  p.csvPanel('rxfifo_s', '$baseurl/network_edev_rxfifo_s.csv','pivoted'),
  p.csvPanel('txfifo_s', '$baseurl/network_edev_txfifo_s.csv','pivoted'),
  p.csvPanel('rxerr_s',  '$baseurl/network_edev_rxerr_s.csv','pivoted'),
  p.csvPanel('txerr_s',  '$baseurl/network_edev_txerr_s.csv','pivoted'),
  p.csvPanel('rxdrop_s', '$baseurl/network_edev_rxdrop_s.csv','pivoted'),
  p.csvPanel('txdrop_s', '$baseurl/network_edev_txdrop_s.csv','pivoted'),
  p.csvPanel('rxfram_s', '$baseurl/network_edev_rxfram_s.csv','pivoted'),
  p.csvPanel('txcarr_s', '$baseurl/network_edev_txcarr_s.csv','pivoted'),
  p.csvPanel('coll_s',   '$baseurl/network_edev_coll_s.csv', 'pivoted'),
];

// --- Network – ICMP ---
local networkIcmpPanels = [
  p.csvPanel('icmp',        '$baseurl/network_icmp.csv'),
  p.csvPanel('icmp6',       '$baseurl/network_icmp6.csv'),
  p.csvPanel('icmp errors', '$baseurl/network_eicmp.csv'),
  p.csvPanel('icmp6 errors','$baseurl/network_eicmp6.csv'),
];

// --- Network – IP ---
local networkIpPanels = [
  p.csvPanel('ip',        '$baseurl/network_ip.csv'),
  p.csvPanel('ip6',       '$baseurl/network_ip6.csv'),
  p.csvPanel('ip errors', '$baseurl/network_eip.csv'),
  p.csvPanel('ip6 errors','$baseurl/network_eip6.csv'),
];

// --- Network – NFS ---
local networkNfsPanels = [
  p.csvPanel('nfs',  '$baseurl/network_nfs.csv'),
  p.csvPanel('nfsd', '$baseurl/network_nfsd.csv'),
];

// --- Network – Software (softnet) ---
local networkSoftPanels = [
  p.csvPanel('Software', '$baseurl/network_soft.csv', showSeries='total/s'),
];

// --- Network – TCP/UDP ---
local networkTcpUdpPanels = [
  p.csvPanel('tcp',        '$baseurl/network_tcp.csv'),
  p.csvPanel('tcp errors', '$baseurl/network_etcp.csv'),
  p.csvPanel('udp',        '$baseurl/network_udp.csv'),
  p.csvPanel('udp6',       '$baseurl/network_udp6.csv'),
  p.csvPanel('sock',       '$baseurl/network_sock.csv'),
  p.csvPanel('sock6',      '$baseurl/network_sock6.csv'),
];

// --- Paging ---
local pagingPanels = [
  p.csvPanel('Mem Management', '$baseurl/paging.csv'),
  p.csvPanel('Reclamation',    '$baseurl/paging.csv'),
  p.csvPanel('NUMA management','$baseurl/paging.csv'),
];

// --- Power ---
local powerPanels = [
  p.csvPanel('MHz', '$baseurl/power_cpu.csv', 'pivoted'),
];

// --- Queue / Load ---
local queuePanels = [
  p.csvPanel('load', '$baseurl/queue.csv'),
  p.csvPanel('load', '$baseurl/queue.csv'),
];

// --- Swap ---
local swapPanels = [
  p.csvPanel('Free',       '$baseurl/swap_util.csv'),
  p.csvPanel('Percentages','$baseurl/swap_util.csv'),
  p.csvPanel('i/o',        '$baseurl/swap.csv'),
];

// --- Task ---
local taskPanels = [
  p.csvPanel('cswch/s', '$baseurl/task.csv'),
  p.csvPanel('proc/s',  '$baseurl/task.csv'),
];

// --- TTY ---
local ttyPanels = [
  p.csvPanel('rcvin/s',   '$baseurl/tty_rcvin_s.csv',   'pivoted'),
  p.csvPanel('txmtin/s',  '$baseurl/tty_txmtin_s.csv',  'pivoted'),
  p.csvPanel('framerr/s', '$baseurl/tty_framerr_s.csv', 'pivoted'),
  p.csvPanel('prtyerr/s', '$baseurl/tty_prtyerr_s.csv', 'pivoted'),
  p.csvPanel('brk/s',     '$baseurl/tty_brk_s.csv',     'pivoted'),
  p.csvPanel('ovrun/s',   '$baseurl/tty_ovrun_s.csv',   'pivoted'),
];

// ---------------------------------------------------------------------------
// Dashboard assembly
// ---------------------------------------------------------------------------
dash.new(dashboardTitle)
+ dash.withUid(dashboardUid)
+ dash.withTimezone('utc')
+ dash.withSchemaVersion(40)
+ dash.graphTooltip.withSharedTooltip()
+ dash.withTemplating({ list: variables })
+ dash.withPanels(
  // CPU panels are top-level (uncollapsed row)
  [p.rowPanel('cpu', collapsed=false)]
  + cpuPanels

  // All other sections are collapsed rows
  + [p.rowPanel('disks',           collapsed=true, innerPanels=diskPanels)]
  + [p.rowPanel('hugepages',       collapsed=true, innerPanels=hugepagesPanels)]
  + [p.rowPanel('inodes',          collapsed=true, innerPanels=inodePanels)]
  + [p.rowPanel('io',              collapsed=true, innerPanels=ioPanels)]
  + [p.rowPanel('interrupts',      collapsed=true, innerPanels=interruptsPanels)]
  + [p.rowPanel('memory',          collapsed=true, innerPanels=memoryPanels)]
  + [p.rowPanel('network_dev',     collapsed=true, innerPanels=networkDevPanels)]
  + [p.rowPanel('network_icmp',    collapsed=true, innerPanels=networkIcmpPanels)]
  + [p.rowPanel('network_ip',      collapsed=true, innerPanels=networkIpPanels)]
  + [p.rowPanel('network_nfs',     collapsed=true, innerPanels=networkNfsPanels)]
  + [p.rowPanel('network_soft',    collapsed=true, innerPanels=networkSoftPanels)]
  + [p.rowPanel('network_tcp/udp', collapsed=true, innerPanels=networkTcpUdpPanels)]
  + [p.rowPanel('paging',          collapsed=true, innerPanels=pagingPanels)]
  + [p.rowPanel('power',           collapsed=true, innerPanels=powerPanels)]
  + [p.rowPanel('queue',           collapsed=true, innerPanels=queuePanels)]
  + [p.rowPanel('swap',            collapsed=true, innerPanels=swapPanels)]
  + [p.rowPanel('task',            collapsed=true, innerPanels=taskPanels)]
  + [p.rowPanel('tty',             collapsed=true, innerPanels=ttyPanels)]
)
