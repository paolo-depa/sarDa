# SAR CSV Grafana Dashboard

This Grafana dashboard visualizes system activity report (SAR) data from CSV files. The data is parsed and displayed using various panels to provide insights into system performance metrics.

## Requirements

- Grafana 7.0 or higher
- `yesoreyeram-infinity-datasource` plugin for Grafana
- CSV files containing SAR data

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

### CPU Usage
- **%user**: Per CPU Percentage utilization that occurred while executing at the user level.
- **%nice**: Per CPU Percentage utilization that occurred while executing at the user level with nice priority.
- **%system**: Per CPU Percentage utilization that occurred while executing at the system level.
- **%iowait**: Per CPU Percentage of time that the CPU or CPUs were idle during which the system had an outstanding disk I/O request.
- **%steal**: Per CPU Percentage of time spent in involuntary wait by the virtual CPU or CPUs while the hypervisor was servicing another virtual processor.
- **%idle**: Per CPU Percentage of time that the CPU or CPUs were idle and the system did not have an outstanding disk I/O request.
- **soft**: Per CPU Percentage utilization that occurred while servicing software interrupts.
- **irq**: Per CPU Percentage utilization that occurred while servicing hardware interrupts.
- **guest**: Per CPU Percentage utilization that occurred while executing at the guest level.
- **steal**: Per CPU Percentage of time spent in involuntary wait by the virtual CPU or CPUs while the hypervisor was servicing another virtual processor.
- **gnice**: Per CPU Percentage utilization that occurred while executing at the guest level with nice priority.
- **nice**: Per CPU Percentage utilization that occurred while executing at the user level with nice priority.

### Disks
- **TPS**: Per device number of transactions per second
- **rkB/s**: Per device amount of read kilobytes per second.
- **wkB/s**: Per device amount of written kilobytes per second.
- **areq-sz**: Per device average request size in kilobytes.
- **aqu-sz**: Per device average queue size.
- **await**: Per device average wait time in milliseconds.
- **%util**: Per device percentage of CPU time during which I/O requests were issued to the device.

### Hugepages
- **hugepages**: Displays metrics such as the percentage of hugepages used and the amount of free hugepages in kilobytes

### Inodes
- **Inode**: Shows metrics such as unused dentries and inodes, file handles, inode handles, and pseudo-terminal handles.

### Interrupts
- **intr/s**: Per CPU number of interrupts per second.

### IO
- **TPS**: Displays overall transactions per second (written/read/discarded) 
- **r/w per sec**: Displays overall read/written data

### Memory Usage
- **Memory**: Displays various memory metrics such as free memory, available memory, used memory, buffers, cached memory, committed memory, active memory, inactive memory, and dirty memory.
- **Memory Percentage**: Shows the percentage of memory used and committed.

### Network Device
- **rxKB_s**: Per device received kilobytes per second.
- **txKB_s**: Per device transmitted kilobytes per second.
- **rxpck_s**: Per device received packets per second.
- **txpck_s**: Per device transmitted packets per second.
- **rxcmp_s**: Per device received compressed packets per second.
- **txcmp_s**: Per device transmitted compressed packets per second.
- **rxmcst_s**: Per device received multicast packets per second.
- **%ifutil**: Per device percentage of interface utilization.
- **rxfifo_s**: Per device received FIFO errors per second.
- **txfifo_s**: Per device transmitted FIFO errors per second.
- **rxerr_s**: Per device received errors per second.
- **txerr_s**: Per device transmitted errors per second.
- **rxdrop_s**: Per device received dropped packets per second.
- **txdrop_s**: Per device transmitted dropped packets per second.
- **rxfram_s**: Per device received frame errors per second.
- **txcarr_s**: Per device transmitted carrier errors per second.
- **coll_s**: Per device collisions per second.

### Network ICMP
- **ICMP/ICMP6**: Displays metrics such as incoming messages per second, outgoing messages per second, incoming echo requests per second, outgoing echo requests per second, incoming echo replies per second, outgoing echo replies per second, incoming timestamp requests per second, outgoing timestamp requests per second, incoming address mask requests per second, and outgoing address mask requests per second (ICMP for IPv4, ICMP6 for IPV6).
- **ICMP/ICMP6 Errors**: Shows metrics such as incoming errors per second, outgoing errors per second, incoming destination unreachable errors per second, outgoing destination unreachable errors per second, incoming parameter problem errors per second, outgoing parameter problem errors per second, incoming source quench errors per second, and outgoing source quench errors per second (ICMP for IPv4, ICMP6 for IPV6)..

### Network IP
- **IP/IP6**: Displays metrics such as received datagrams per second, forwarded datagrams per second, incoming datagrams discarded per second, outgoing datagrams discarded per second, and reassembled fragments per second (IP for IPv4, IP6 for IPV6).
- **IP/IP6 Errors**: Shows metrics such as header errors per second, address errors per second, unknown protocol errors per second, incoming packets discarded per second, outgoing packets discarded per second, and reassembly failures per second (ICMP for IPv4, ICMP6 for IPV6).

### Network NFS
- **NFS**: Displays metrics such as retransmissions per second, reads per second, writes per second, accesses per second, and get attributes per second.
- **NFSD**: Shows metrics such as server calls per second, bad calls per second, packets per second, UDP packets per second, TCP packets per second, cache hits per second, cache misses per second, server reads per second, server writes per second, server accesses per second, and server get attributes per second.

### Network TCP/UDP
- **TCP**: Displays metrics such as active connections per second, passive connections per second, segments received per second, and segments sent per second.
- **TCP Errors**: Shows metrics such as failed connection attempts per second, reset connections per second, retransmitted segments per second, incoming segments with errors per second, and outgoing segments with resets per second.
- **UDP/UDP6**: Displays metrics such as received datagrams per second, sent datagrams per second, no port errors per second, and received datagrams with errors per second (UDP for IPv4, UDP6 for IPV6)..
- **Sockets/Socket6**: Shows metrics such as total sockets, TCP sockets, UDP sockets, raw sockets, IP fragments, and TCP time-wait sockets (Sockets for IPv4, Sockets6 for IPV6).


### Paging
- **Paging**: Displays metrics such as pages paged in per second, pages paged out per second, major faults per second, pages freed per second, pages scanned by kswapd per second, pages scanned directly per second, pages stolen per second, pages promoted per second, pages demoted per second, and total faults per second.

### Power
- **Power**: Displays CPU frequency in MHz.

### Queue
- **Queue**: Shows metrics such as process list size, run queue size, and blocked processes.
- **Load Average**: Displays load average metrics for 1 minute, 5 minutes, and 15 minutes.

### Swap
- **Swap Utilization**: Displays metrics such as free swap space, used swap space, cached swap space, percentage of swap space used, and percentage of cached swap space.
- **Swap I/O**: Shows metrics such as pages swapped in per second and pages swapped out per second.

### Task
- **Task**: Displays metrics such as processes created per second and context switches per second.

### TTY
- **TTY**: Shows metrics such as received characters per second, transmitted characters per second, framing errors per second, parity errors per second, break conditions per second, and overrun errors per second.
