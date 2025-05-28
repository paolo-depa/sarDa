# SAR CSV Grafana Dashboard

This Grafana dashboard visualizes system activity report (SAR) data from CSV files.
The data is parsed and displayed using various panels to provide insights into system performance metrics.

## Requirements

- Grafana 7.0 or higher
- `yesoreyeram-infinity-datasource` plugin for Grafana
- CSV files containing SAR data, generated using the [sar-parser.py](../bin/sar-parser.py) tool

## Configuration

1. Open Grafana and navigate to **Connections** > **Connect data ** > **Data Sources**.
2. Add a new data source and select **Infinity** from the list of available data sources.
3. Configure the data source with the following settings:
   - **Name**: `SAR CSV Data Source`
   - **URL**: leave empty

## Usage

1. Import the dashboard JSON file into Grafana:
   - Go to **Dashboard** > **New button dropdown** > **Import**.
   - Upload the `sar-csv.json` file or paste its JSON content.
2. Select the `SAR CSV Data Source` as the data source for the dashboard.

## Panels

### CPU
This section visualizes per-CPU metrics from the `per_cpu_*.csv` files.
- **idle**: Per-CPU percentage of time the CPU was idle with no outstanding I/O requests.
- **iowait**: Per-CPU percentage of time the CPU was idle waiting for I/O completion.
- **system**: Per-CPU percentage utilization while executing in kernel space.
- **user**: Per-CPU percentage utilization while executing in user space.
- **irq**: Per-CPU percentage utilization while servicing hardware interrupts.
- **soft**: Per-CPU percentage utilization while servicing software interrupts.
- **nice**: Per-CPU percentage utilization while executing user-level processes with nice priority.
- **gnice**: Per-CPU percentage utilization while executing guest processes with nice priority.
- **steal**: Per-CPU percentage of time spent in involuntary wait by a virtual CPU while the hypervisor was servicing another virtual processor.
- **guest**: Per-CPU percentage utilization while executing a guest virtual machine.

### Disks
- **%util**: Per-device percentage of CPU time during which I/O requests were issued to the device.
- **await**: Per-device average wait time (queue + service) for I/O requests in milliseconds.
- **tps**: Per-device number of transactions (requests) per second.
- **wkB_s**: Per-device amount of written kilobytes per second.
- **rkB_s**: Per-device amount of read kilobytes per second.
- **areq-sz**: Per-device average request size in kilobytes.
- **aqu-sz**: Per-device average queue size.

### Hugepages
- **hugepages**: Displays hugepage usage metrics including free (`kbhugfree`), used (`kbhugused`), percentage used (`%hugused`), reserved (`kbhugrsvd`), and surplus (`kbhugsurp`) hugepages in kilobytes.

### Inodes
- **inode**: Shows inode and file handle usage metrics including unused dentries (`dentunusd`), file handles (`file-nr`), inode handles (`inode-nr`), and pseudo-terminal handles (`pty-nr`).

### Interrupts
- **intr/s**: Per-CPU number of interrupts per second.

### IO
- **TPS**: Displays overall I/O transactions per second (`tps`), including read (`rtps`), write (`wtps`), and discard (`dtps`) transactions.
- **r/w per sec**: Displays overall I/O data transfer rates in kilobytes per second, including read (`bread/s`), written (`bwrtn/s`), and discarded (`bdscd/s`) data.

### Memory Usage
- **General**: Displays general memory usage metrics including free (`kbmemfree`), available (`kbavail`), used (`kbmemused`), and virtual memory used (`kbvmused`) in kilobytes.
- **Active/Inactive**: Displays active (`kbactive`) and inactive (`kbinact`) memory in kilobytes.
- **Kernel mem**: Displays kernel memory usage metrics including buffers (`kbbuffers`), slab (`kbslab`), kernel stack (`kbkstack`), and page table (`kbpgtbl`) memory in kilobytes.
- **Cached/Committed/Anon/Dirty**: Displays cached (`kbcached`), committed (`kbcommit`), dirty (`kbdirty`), and anonymous pages (`kbanonpg`) memory in kilobytes.
- **memory perc.**: Shows the percentage of memory used (`%=memused`) and committed (`%commit`).

### Network Device
- **rxkB_s**: Per-device received kilobytes per second.
- **txkB_s**: Per-device transmitted kilobytes per second.
- **rxpck_s**: Per-device received packets per second.
- **txpck_s**: Per-device transmitted packets per second.
- **rxcmp_s**: Per-device received compressed packets per second.
- **txcmp_s**: Per-device transmitted compressed packets per second.
- **rxmcst_s**: Per-device received multicast packets per second.
- **%ifutil**: Per-device percentage of interface utilization.
- **rxfifo_s**: Per-device received FIFO errors per second.
- **txfifo_s**: Per-device transmitted FIFO errors per second.
- **rxerr_s**: Per-device received errors per second.
- **txerr_s**: Per-device transmitted errors per second.
- **rxdrop_s**: Per-device received dropped packets per second.
- **txdrop_s**: Per-device transmitted dropped packets per second.
- **rxfram_s**: Per-device received frame errors per second.
- **txcarr_s**: Per-device transmitted carrier errors per second.
- **coll_s**: Per-device collisions per second.

### Network ICMP
- **icmp**: Displays IPv4 ICMP metrics including incoming/outgoing messages, echo requests/replies, timestamp requests, and address mask requests per second.
- **icmp6**: Displays IPv6 ICMP metrics including incoming/outgoing messages, echo requests/replies, timestamp requests, and address mask requests per second.
- **icmp errors**: Displays IPv4 ICMP error metrics including incoming/outgoing errors, destination unreachable, parameter problem, source quench, and redirect errors per second.
- **icmp6 errors**: Displays IPv6 ICMP error metrics including incoming/outgoing errors, destination unreachable, parameter problem, source quench, and redirect errors per second.

### Network IP
- **ip**: Displays IPv4 IP metrics including received datagrams, forwarded datagrams, incoming/outgoing discarded datagrams, and reassembled/fragmented packets per second.
- **ip6**: Displays IPv6 IP metrics including received datagrams, forwarded datagrams, incoming/outgoing discarded datagrams, and reassembled/fragmented packets per second.
- **ip errors**: Displays IPv4 IP error metrics including header errors, address errors, unknown protocol errors, incoming/outgoing discarded packets, and reassembly/fragmentation failures per second.
- **ip6 errors**: Displays IPv6 IP error metrics including header errors, address errors, unknown protocol errors, incoming/outgoing discarded packets, and reassembly/fragmentation failures per second.

### Network NFS
- **nfs**: Displays NFS client metrics including retransmissions, reads, writes, accesses, and get attributes per second.
- **nfsd**: Displays NFS server metrics including server calls, bad calls, packets, UDP/TCP packets, cache hits/misses, and server reads/writes/accesses/get attributes per second.

### Network soft
- **Software**: Displays software-based network processing statistics including total packets processed per second (`total/s`), dropped packets per second (`dropd/s`), squeezed packets per second (`squeezd/s`), receive RPS (`rx_rps/s`), and flow limit count per second (`flw_lim/s`).

### Network TCP/UDP
- **tcp**: Displays TCP connection metrics including active/passive connections, and segments received/sent per second.
- **tcp errors**: Displays TCP error metrics including failed connection attempts, reset connections, retransmitted segments, incoming segments with errors, and outgoing segments with resets per second.
- **udp**: Displays IPv4 UDP metrics including received/sent datagrams, no port errors, and received datagrams with errors per second.
- **udp6**: Displays IPv6 UDP metrics including received/sent datagrams, no port errors, and received datagrams with errors per second.
- **sock**: Shows IPv4 socket usage metrics including total sockets, TCP, UDP, raw sockets, IP fragments, and TCP time-wait sockets.
- **sock6**: Shows IPv6 socket usage metrics including TCP, UDP, raw sockets, and IP fragments.

### Paging
- **Mem Management**: Displays memory management metrics including pages paged in/out, pages freed, faults, and major faults per second.
- **Reclamation**: Displays page reclamation metrics including pages scanned by kswapd/directly, and pages stolen per second.
- **NUMA management**: Displays NUMA memory management metrics including pages promoted and demoted per second.

### Power
- **MHz**: Displays CPU frequency in MHz.

### Queue
- **load**: Displays queue and load metrics including process list size (`plist-sz`), run queue size (`runq-sz`), and blocked processes (`blocked`).
- **load**: Displays load average metrics for 1 minute (`ldavg-1`), 5 minutes (`ldavg-5`), and 15 minutes (`ldavg-15`).

### Swap
- **Free**: Displays swap space usage metrics including free (`kbswpfree`), used (`kbswpused`), and cached (`kbswpcad`) swap space in kilobytes.
- **Percentages**: Shows the percentage of swap space used (`%swpused`) and cached (`%swpcad`).
- **i/o**: Displays swap I/O metrics including pages swapped in (`pswpin/s`) and pages swapped out (`pswpout/s`) per second.

### Task
- **cswch/s**: Displays the number of context switches per second.
- **proc/s**: Displays the number of processes created per second.

### TTY
- **rcvin/s**: Displays the number of received characters per second from TTYs.
- **txmtin/s**: Displays the number of transmitted characters per second to TTYs.
- **framerr_s**: Displays the number of framing errors per second on TTYs.
- **prtyerr_s**: Displays the number of parity errors per second on TTYs.
- **brk_s**: Displays the number of break conditions per second on TTYs.
- **ovrun_s**: Displays the number of overrun errors per second on TTYs.
