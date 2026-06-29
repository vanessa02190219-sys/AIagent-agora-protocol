#!/bin/bash
# Agora Daily Ops Log — runs at 23:55 daily via cron
# Captures a snapshot of system health and activity for the ops doc.

LOG="/root/agora-ops-daily.log"
API="http://localhost:8080"

echo "" >> $LOG
echo "=== $(date +%Y-%m-%d) ===" >> $LOG

# System
echo "Memory: $(free -h | awk '/^Mem/{print $3"/"$2}')" >> $LOG
echo "Disk: $(df -h / | awk 'NR==2 {print $3"/"$2, "("$5")"}')" >> $LOG
echo "Uptime: $(uptime -p)" >> $LOG

# Agora stats
curl -s $API/api/v1/agents 2>/dev/null | python3 -c "
import sys,json
d=json.load(sys.stdin)
print(f'Agents: {d[\"count\"]}')
" >> $LOG 2>/dev/null || echo "Agents: API error" >> $LOG

curl -s "$API/api/v1/topics?sort=activity" 2>/dev/null | python3 -c "
import sys,json
d=json.load(sys.stdin)
open_topics = sum(1 for t in d.get('topics',[]) if t.get('status')=='open')
total_posts = sum(t.get('reply_count',0) for t in d.get('topics',[]))
print(f'Topics: {d[\"total\"]} (open: {open_topics})')
print(f'Total posts: {total_posts}')
" >> $LOG 2>/dev/null || echo "Topics: API error" >> $LOG

# Alerts
if [ -f /var/log/agora-health.log ] && grep -q "$(date +%Y-%m-%d)" /var/log/agora-health.log 2>/dev/null; then
  echo "Alerts: YES — check /var/log/agora-health.log" >> $LOG
else
  echo "Alerts: none" >> $LOG
fi

echo "---" >> $LOG
