// SPDX-License-Identifier: GPL-2.0-only
/*
 * rpmsg_wwan_ctrl_multi — expose additional Qualcomm SMD control channels as
 * WWAN QMI ports.
 *
 * Derived from the in-tree driver:
 *   drivers/net/wwan/rpmsg_wwan_ctrl.c
 *   Copyright (c) 2021, Stephan Gerhold <stephan@gerhold.net>
 *
 * ---------------------------------------------------------------------------
 * Why this exists
 * ---------------------------------------------------------------------------
 * The upstream driver only claims three channels:
 *
 *   { "DATA5_CNTL", WWAN_PORT_QMI }   -> becomes wwan0qmi0
 *   { "DATA4",      WWAN_PORT_AT  }   -> becomes wwan0at1
 *   { "DATA1",      WWAN_PORT_AT  }   -> becomes wwan0at0
 *
 * On a Qualcomm SoC the modem firmware also advertises a range of spare
 * DATA<n>_CNTL channels (DATA6..DATA9, DATA12.., DATA40..). Those are unusable
 * upstream: forcing one onto the driver with `driver_override` makes the probe
 * pass `rpdev->id.driver_data` == 0 (WWAN_PORT_UNKNOWN) into wwan_create_port(),
 * so the kernel publishes it as a generic/AT-looking port whose data path is not
 * a real QMI endpoint. Observed symptoms on such a port: CTL queries answer
 * (qmicli --get-service-version-info lists wds/dms), but allocating a WDS client
 * is unreliable ("CID allocation failed ... endpoint hangup") and
 * --wds-start-network wedges the baseband.
 *
 * This driver adds the spare channels to the ID table with the correct
 * WWAN_PORT_QMI type, so each one is published as a proper QMI port
 * (wwan0qmi1, wwan0qmi2, ...).
 *
 * ---------------------------------------------------------------------------
 * What it is used for
 * ---------------------------------------------------------------------------
 * ModemManager owns the primary QMI port (from DATA5_CNTL) to run the normal
 * mobile-data bearer. Creating a second data session for IMS/VoLTE on that same
 * port fails with `interface-in-use-config-match`, or — on MSM8916-class
 * firmware — crashes the modem while activating the IMS PDP context. Giving IMS
 * its own physical QMI endpoint avoids both.
 *
 * ---------------------------------------------------------------------------
 * Coexistence with the in-tree driver
 * ---------------------------------------------------------------------------
 * This module deliberately does NOT claim DATA1/DATA4/DATA5_CNTL, so it can be
 * loaded alongside the in-tree rpmsg_wwan_ctrl without fighting over the ports
 * ModemManager already uses. Load order does not matter.
 *
 * ---------------------------------------------------------------------------
 * Extending
 * ---------------------------------------------------------------------------
 * To expose more channels, add entries to rpmsg_wwan_multi_id_table below. Use
 * WWAN_PORT_QMI for the _CNTL control channels; use WWAN_PORT_AT only for
 * channels the firmware really drives as AT consoles. List the channels your
 * firmware offers by reading the "name" attribute of every device under
 * /sys/bus/rpmsg/devices.
 */

#include <linux/kernel.h>
#include <linux/module.h>
#include <linux/platform_device.h>
#include <linux/rpmsg.h>
#include <linux/skbuff.h>
#include <linux/wwan.h>

struct rpmsg_wwan_dev {
	/* Lower level is a rpmsg dev, upper level is a wwan port */
	struct rpmsg_device *rpdev;
	struct wwan_port *wwan_port;
	struct rpmsg_endpoint *ept;
};

static int rpmsg_wwan_multi_callback(struct rpmsg_device *rpdev,
				     void *buf, int len, void *priv, u32 src)
{
	struct rpmsg_wwan_dev *rpwwan = priv;
	struct sk_buff *skb;

	skb = alloc_skb(len, GFP_ATOMIC);
	if (!skb)
		return -ENOMEM;

	skb_put_data(skb, buf, len);
	wwan_port_rx(rpwwan->wwan_port, skb);
	return 0;
}

static int rpmsg_wwan_multi_start(struct wwan_port *port)
{
	struct rpmsg_wwan_dev *rpwwan = wwan_port_get_drvdata(port);
	struct rpmsg_channel_info chinfo = {
		.src = rpwwan->rpdev->src,
		.dst = RPMSG_ADDR_ANY,
	};

	strscpy(chinfo.name, rpwwan->rpdev->id.name, sizeof(chinfo.name));
	rpwwan->ept = rpmsg_create_ept(rpwwan->rpdev, rpmsg_wwan_multi_callback,
				       rpwwan, chinfo);
	if (!rpwwan->ept)
		return -EREMOTEIO;

	return 0;
}

static void rpmsg_wwan_multi_stop(struct wwan_port *port)
{
	struct rpmsg_wwan_dev *rpwwan = wwan_port_get_drvdata(port);

	rpmsg_destroy_ept(rpwwan->ept);
	rpwwan->ept = NULL;
}

static int rpmsg_wwan_multi_tx(struct wwan_port *port, struct sk_buff *skb)
{
	struct rpmsg_wwan_dev *rpwwan = wwan_port_get_drvdata(port);
	int ret;

	ret = rpmsg_trysend(rpwwan->ept, skb->data, skb->len);
	if (ret)
		return ret;

	consume_skb(skb);
	return 0;
}

static int rpmsg_wwan_multi_tx_blocking(struct wwan_port *port,
					struct sk_buff *skb)
{
	struct rpmsg_wwan_dev *rpwwan = wwan_port_get_drvdata(port);
	int ret;

	ret = rpmsg_send(rpwwan->ept, skb->data, skb->len);
	if (ret)
		return ret;

	consume_skb(skb);
	return 0;
}

static __poll_t rpmsg_wwan_multi_tx_poll(struct wwan_port *port,
					 struct file *filp, poll_table *wait)
{
	struct rpmsg_wwan_dev *rpwwan = wwan_port_get_drvdata(port);

	return rpmsg_poll(rpwwan->ept, filp, wait);
}

static const struct wwan_port_ops rpmsg_wwan_multi_pops = {
	.start = rpmsg_wwan_multi_start,
	.stop = rpmsg_wwan_multi_stop,
	.tx = rpmsg_wwan_multi_tx,
	.tx_blocking = rpmsg_wwan_multi_tx_blocking,
	.tx_poll = rpmsg_wwan_multi_tx_poll,
};

static struct device *rpmsg_wwan_find_parent(struct device *dev)
{
	/* Select the first platform device as parent for the WWAN ports. On
	 * Qualcomm platforms this is the platform device representing the modem
	 * remote processor, which is also what makes the resulting port share a
	 * sysfs ancestor with the primary QMI port — userspace relies on that
	 * to pair endpoints belonging to the same baseband.
	 */
	for (dev = dev->parent; dev; dev = dev->parent) {
		if (dev_is_platform(dev))
			return dev;
	}
	return NULL;
}

static int rpmsg_wwan_multi_probe(struct rpmsg_device *rpdev)
{
	struct rpmsg_wwan_dev *rpwwan;
	struct wwan_port *port;
	struct device *parent;

	parent = rpmsg_wwan_find_parent(&rpdev->dev);
	if (!parent)
		return -ENODEV;

	rpwwan = devm_kzalloc(&rpdev->dev, sizeof(*rpwwan), GFP_KERNEL);
	if (!rpwwan)
		return -ENOMEM;

	rpwwan->rpdev = rpdev;
	dev_set_drvdata(&rpdev->dev, rpwwan);

	/* id.driver_data carries the wwan port type for the matched channel. */
	port = wwan_create_port(parent, rpdev->id.driver_data,
				&rpmsg_wwan_multi_pops, NULL, rpwwan);
	if (IS_ERR(port))
		return PTR_ERR(port);

	rpwwan->wwan_port = port;

	dev_info(&rpdev->dev, "exposed %s as a WWAN port (type %lu)\n",
		 rpdev->id.name, rpdev->id.driver_data);

	return 0;
}

static void rpmsg_wwan_multi_remove(struct rpmsg_device *rpdev)
{
	struct rpmsg_wwan_dev *rpwwan = dev_get_drvdata(&rpdev->dev);

	wwan_remove_port(rpwwan->wwan_port);
}

/*
 * Spare Qualcomm SMD control channels published as QMI ports.
 *
 * DATA1 / DATA4 / DATA5_CNTL are intentionally omitted: they are claimed by the
 * in-tree rpmsg_wwan_ctrl and back the ports ModemManager already uses.
 *
 * Keep this list SHORT. Every entry the firmware offers is probed at module
 * load, and each one attaches a WWAN port. Claiming the full DATA6..DATA40 range
 * published ~14 ports at once on the reference device, which both floods
 * ModemManager with ports it might try to manage and hammers the modem firmware
 * during attach. One spare endpoint per baseband is what IMS needs; DATA7 is
 * kept only as a fallback when DATA6 is unavailable.
 *
 * If you need more, add them one at a time and verify the modem stays up
 * (`mmcli -L`) after each addition.
 */
static const struct rpmsg_device_id rpmsg_wwan_multi_id_table[] = {
	{ .name = "DATA6_CNTL",  .driver_data = WWAN_PORT_QMI },
	{ .name = "DATA7_CNTL",  .driver_data = WWAN_PORT_QMI },
	{},
};
MODULE_DEVICE_TABLE(rpmsg, rpmsg_wwan_multi_id_table);

static struct rpmsg_driver rpmsg_wwan_multi_driver = {
	.drv.name = "rpmsg_wwan_ctrl_multi",
	.id_table = rpmsg_wwan_multi_id_table,
	.probe = rpmsg_wwan_multi_probe,
	.remove = rpmsg_wwan_multi_remove,
};
module_rpmsg_driver(rpmsg_wwan_multi_driver);

MODULE_LICENSE("GPL v2");
MODULE_DESCRIPTION("RPMSG WWAN CTRL Driver (spare QMI channels for IMS)");
MODULE_AUTHOR("Stephan Gerhold <stephan@gerhold.net>");
