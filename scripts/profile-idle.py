#!/usr/bin/env python3
"""Sample an identified native app and WebKit processes without changing them."""
import argparse
import json
import statistics
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument('pids', type=int, nargs='+', help='Main app and its identified WebKit PIDs')
parser.add_argument('--seconds', type=int, default=30)
parser.add_argument('--workspace', default='')
args = parser.parse_args()
if args.seconds < 1 or any(pid <= 0 for pid in args.pids):
    parser.error('require positive PIDs and duration')

identified = set(args.pids)
samples = []
for sample in range(args.seconds):
    output = subprocess.run(
        ['ps', '-axo', 'pid=,ppid=,rss=,%cpu='],
        check=True, capture_output=True, text=True,
    ).stdout
    processes = {}
    for line in output.splitlines():
        pid, parent, rss, cpu = line.split()
        processes[int(pid)] = (int(parent), int(rss), float(cpu))
    if not identified.issubset(processes):
        parser.error('an identified process exited; restart the measurement with current PIDs')
    included = identified.copy()
    while True:
        children = {pid for pid, (parent, _, _) in processes.items() if parent in included}
        if children.issubset(included):
            break
        included.update(children)
    samples.append({
        'rssMiB': sum(processes[pid][1] for pid in included) / 1024,
        'cpuPercent': sum(processes[pid][2] for pid in included),
        'processes': len(included),
    })
    if sample + 1 < args.seconds:
        time.sleep(1)

print(json.dumps({
    'pids': sorted(identified),
    'samples': len(samples),
    'meanRssMiB': statistics.mean(sample['rssMiB'] for sample in samples),
    'peakRssMiB': max(sample['rssMiB'] for sample in samples),
    'meanReportedCpuPercent': statistics.mean(sample['cpuPercent'] for sample in samples),
    'peakProcesses': max(sample['processes'] for sample in samples),
    'workspace': args.workspace,
    'includes': 'identified main/WebKit processes and their descendants',
    'cpuMethod': 'mean of ps reported percent CPU, sampled every second',
}, indent=2))
