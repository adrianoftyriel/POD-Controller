#!/bin/sh
# Install pod-controller on a Raspberry Pi (tested: Zero 2 W, Debian 13,
# alongside Companion Satellite). Run on the Pi from a directory holding the
# aarch64 pod-cli binary, the ui/ directory and the files next to this
# script. It starts the service but doesn't enable it at boot; run
# `sudo systemctl enable pod-controller` for that.
set -e
here=$(dirname "$0")
sudo install -d -m 755 /opt/pod-controller
sudo install -m 755 pod-cli /opt/pod-controller/pod-cli
sudo rm -rf /opt/pod-controller/ui
sudo cp -r ui /opt/pod-controller/ui
id podctl >/dev/null 2>&1 || sudo useradd --system --no-create-home --shell /usr/sbin/nologin podctl
sudo install -m 644 "$here/70-pod-x3.rules" /etc/udev/rules.d/70-pod-x3.rules
sudo install -m 644 "$here/pod-x3-no-podhd.conf" /etc/modprobe.d/pod-x3-no-podhd.conf
sudo install -m 644 "$here/pod-controller.service" /etc/systemd/system/pod-controller.service
sudo udevadm control --reload
sudo udevadm trigger --subsystem-match=usb --attr-match=idVendor=0e41
sudo systemctl daemon-reload
sudo systemctl restart pod-controller
systemctl --no-pager status pod-controller | head -5
