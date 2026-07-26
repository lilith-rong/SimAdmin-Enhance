# Out-of-tree build for rpmsg_wwan_ctrl_multi.
#
# Native build on the target device (needs gcc + make + kernel headers):
#   make
#   make install      # copies to /lib/modules/$(uname -r)/extra/simadmin and runs depmod
#   make load         # modprobe
#
# Cross build from a host:
#   make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- \
#        KDIR=/path/to/linux-headers-6.17.0-rc6-lkiuyu-compile+
#
# KDIR defaults to the running kernel's build tree.

obj-m += rpmsg_wwan_ctrl_multi.o

KVER  ?= $(shell uname -r)
KDIR  ?= /lib/modules/$(KVER)/build
PWD   := $(shell pwd)
EXTRA_DIR := /lib/modules/$(KVER)/extra/simadmin

all:
	$(MAKE) -C $(KDIR) M=$(PWD) modules

clean:
	$(MAKE) -C $(KDIR) M=$(PWD) clean

install: all
	install -d $(EXTRA_DIR)
	install -m 0644 rpmsg_wwan_ctrl_multi.ko $(EXTRA_DIR)/
	depmod -a $(KVER)
	@echo "Installed to $(EXTRA_DIR)/rpmsg_wwan_ctrl_multi.ko"

load:
	modprobe rpmsg_wwan_ctrl_multi || insmod $(EXTRA_DIR)/rpmsg_wwan_ctrl_multi.ko

unload:
	-rmmod rpmsg_wwan_ctrl_multi

.PHONY: all clean install load unload
