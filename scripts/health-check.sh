#!/bin/bash
# Agora Health Monitor — runs every 5min via cron
# Alerts on: high memory, low disk, process down, API unresponsive

LOG=/var/log/agora-health.log
API=http://localhost:8080/health

MEM_THRESHOLD=85   # alert if memory > 85%
DISK_THRESHOLD=85  # alert if disk > 85%

now=$(date -Iseconds)

# 1. Memory
mem_pct=$(free | awk '/^Mem/ {printf "%.0f", $3/$2*100}')
if [ "$mem_pct" -gt "$MEM_THRESHOLD" ]; then
  echo "$now ALERT: Memory ${mem_pct}% (threshold ${MEM_THRESHOLD}%)" >> $LOG
fi

# 2. Disk
disk_pct=$(df / | awk 'NR==2 {gsub(/%/,""); print $5}')
if [ "$disk_pct" -gt "$DISK_THRESHOLD" ]; then
  echo "$now ALERT: Disk ${disk_pct}% (threshold ${DISK_THRESHOLD}%)" >> $LOG
fi

# 3. Process
if ! pgrep -x agora-api > /dev/null; then
  echo "$now ALERT: agora-api process not running!" >> $LOG
fi

# 4. API responsiveness
if ! curl -sf -o /dev/null -m 5 $API; then
  echo "$now ALERT: API health check failed" >> $LOG
fi

# Trim log to last 1000 lines
tail -n 1000 $LOG > /tmp/agora-health.tmp && mv /tmp/agora-health.tmp $LOG
