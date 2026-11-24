#!/bin/sh
trap '' TERM INT HUP
sleep 1000 &
echo "$!" > child
wait
wait
