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

### Disks
- **TPS**: Displays transactions per second for each device.
- **rkB/s**: Shows the read kilobytes per second for each device.
- **wkB/s**: Shows the write kilobytes per second for each device.
- **dkB/s**: Displays the discard kilobytes per second for each device.
- **areq-sz**: Average request size in kilobytes.
- **aqu-sz**: Average queue size.
- **await**: Average wait time in milliseconds.
- **%util**: Percentage of CPU time during which I/O requests were issued to the device.

### IO
- **TPS**: Displays transactions per second (written/read/discarded) overall
- **r/w per sec**: Displays written/read/discarded data overall

### Hugepages
- **Hugepages**: Displays metrics such as free hugepages, used hugepages, reserved hugepages, and surplus hugepages.

### Inodes
- **Inodes**: Shows metrics such as unused dentries and inodes, file handles, inode handles, and pseudo-terminal handles.

### Memory Usage
- **Memory**: Displays various memory metrics such as free memory, available memory, used memory, buffers, cached memory, committed memory, active memory, inactive memory, and dirty memory.
- **Memory Percentage**: Shows the percentage of memory used and committed.

### CPU Usage
- **%user**: Percentage of CPU utilization that occurred while executing at the user level.
- **%nice**: Percentage of CPU utilization that occurred while executing at the user level with nice priority.
- **%system**: Percentage of CPU utilization that occurred while executing at the system level.
- **%iowait**: Percentage of time that the CPU or CPUs were idle during which the system had an outstanding disk I/O request.
- **%steal**: Percentage of time spent in involuntary wait by the virtual CPU or CPUs while the hypervisor was servicing another virtual processor.
- **%idle**: Percentage of time that the CPU or CPUs were idle and the system did not have an outstanding disk I/O request.

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

### Network Device
- **Network Device**: Displays metrics such as received packets per second, transmitted packets per second, received kilobytes per second, transmitted kilobytes per second, and interface utilization percentage.
- **Network Errors**: Shows metrics such as receive errors per second, transmit errors per second, receive drops per second, transmit drops per second, and FIFO errors per second.

### Network IP
- **IP**: Displays metrics such as received datagrams per second, forwarded datagrams per second, incoming datagrams discarded per second, outgoing datagrams discarded per second, and reassembled fragments per second.
- **IP Errors**: Shows metrics such as header errors per second, address errors per second, unknown protocol errors per second, incoming packets discarded per second, outgoing packets discarded per second, and reassembly failures per second.

### Network ICMP
- **ICMP**: Displays metrics such as incoming messages per second, outgoing messages per second, incoming echo requests per second, outgoing echo requests per second, incoming echo replies per second, outgoing echo replies per second, incoming timestamp requests per second, outgoing timestamp requests per second, incoming address mask requests per second, and outgoing address mask requests per second.
- **ICMP Errors**: Shows metrics such as incoming errors per second, outgoing errors per second, incoming destination unreachable errors per second, outgoing destination unreachable errors per second, incoming parameter problem errors per second, outgoing parameter problem errors per second, incoming source quench errors per second, and outgoing source quench errors per second.

### Network TCP/UDP
- **TCP**: Displays metrics such as active connections per second, passive connections per second, segments received per second, and segments sent per second.
- **TCP Errors**: Shows metrics such as failed connection attempts per second, reset connections per second, retransmitted segments per second, incoming segments with errors per second, and outgoing segments with resets per second.
- **UDP**: Displays metrics such as received datagrams per second, sent datagrams per second, no port errors per second, and received datagrams with errors per second.
- **UDP6**: Displays similar metrics as UDP but for IPv6.
- **Sockets**: Shows metrics such as total sockets, TCP sockets, UDP sockets, raw sockets, IP fragments, and TCP time-wait sockets.
- **Sockets6**: Displays similar metrics as Sockets but for IPv6.

### Network NFS
- **NFS**: Displays metrics such as retransmissions per second, reads per second, writes per second, accesses per second, and get attributes per second.
- **NFSD**: Shows metrics such as server calls per second, bad calls per second, packets per second, UDP packets per second, TCP packets per second, cache hits per second, cache misses per second, server reads per second, server writes per second, server accesses per second, and server get attributes per second.