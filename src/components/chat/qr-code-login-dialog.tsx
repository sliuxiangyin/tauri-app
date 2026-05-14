// QRCodeLoginDialog.tsx
import React, { useState, useEffect, useRef } from "react";
import { Loader2 } from "lucide-react";
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { toast } from "sonner";
import { fetchStartQr, fetchWaitQr } from "@/lib/weixing-api";
import encodeQR from "qr";

interface QRCodeLoginDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    onLoginSuccess?: () => void | Promise<void>;
}

export const QRCodeLoginDialog: React.FC<QRCodeLoginDialogProps> = ({
                                                                        open,
                                                                        onOpenChange,
                                                                        onLoginSuccess,
                                                                    }) => {
    const [isPolling, setIsPolling] = useState(false);
    const [isLoading, setIsLoading] = useState(false);
    const [qrSvg, setQrSvg] = useState<string>("");
    const pollingIntervalRef = useRef<number | null>(null);
    const currentSessionKeyRef = useRef<string>("");
    const qrContainerRef = useRef<HTMLDivElement>(null);

    const stopPolling = () => {
        if (pollingIntervalRef.current) {
            clearInterval(pollingIntervalRef.current);
            pollingIntervalRef.current = null;
        }
        setIsPolling(false);
    };

    const startPolling = async (sessionKey: string) => {
        if (pollingIntervalRef.current) {
            stopPolling();
        }

        setIsPolling(true);
        currentSessionKeyRef.current = sessionKey;

        const poll = async () => {
            try {
                const result = await fetchWaitQr(sessionKey);
                console.log("Poll result:", result);

                if (result.connected  ) {
                    stopPolling();
                    toast.success("微信登录成功！", {
                        description: "您已成功连接微信账号",
                        duration: 3000,
                    });

                    if (onLoginSuccess) {
                        await onLoginSuccess();
                    }

                    onOpenChange(false);
                }
            } catch (error) {
                console.error("Polling error:", error);
            }
        };

        await poll();
        pollingIntervalRef.current = window.setInterval(poll, 3000);
    };

    // 从 URL 中提取二维码数据
    const extractQRData = (url: string): string => {
        try {
            const urlObj = new URL(url);
            // 尝试获取 qrcode 参数
            const qrcode = urlObj.searchParams.get("qrcode");
            if (qrcode) {
                return qrcode;
            }
            // 如果没有 qrcode 参数，可能整个 URL 就是需要编码的内容
            return url;
        } catch {
            return url;
        }
    };

    // 生成 SVG 二维码
    const generateQRCode = async (url: string) => {
        setIsLoading(true);
        try {
            const qrData = extractQRData(url);
            console.log("QR Data to encode:", qrData);

            // 生成 SVG 二维码，scale 控制大小，border 控制边框
            const svg = encodeQR(qrData, "svg") as unknown  as SVGElement;
            console.log(svg);
            // 将 SVG 元素转换为字符串
            const serializer = new XMLSerializer();
            const svgString = serializer.serializeToString(svg);
            setQrSvg(svgString);
        } catch (error) {
            console.error("Failed to generate QR code:", error);
            toast.error("生成二维码失败", {
                description: error instanceof Error ? error.message : "请重试",
            });
            onOpenChange(false);
        } finally {
            setIsLoading(false);
        }
    };

    const initQRCode = async () => {
        try {
            const result = await fetchStartQr();
            console.log("QR URL:", result.qrcodeUrl);
            // 用获取到的 URL 生成二维码
            await generateQRCode(result.qrcodeUrl??'');
            await startPolling(result.sessionKey);
        } catch (error) {
            console.error("Failed to start QR login:", error);
            toast.error("启动微信登录失败", {
                description: error instanceof Error ? error.message : "请重试",
            });
            onOpenChange(false);
        }
    };

    useEffect(() => {
        if (open) {
            initQRCode();
        }

        return () => {
            if (!open) {
                stopPolling();
                setQrSvg("");
            }
        };
    }, [open]);

    useEffect(() => {
        return () => {
            stopPolling();
        };
    }, []);

    // 将 SVG 字符串插入 DOM
    useEffect(() => {
        if (qrSvg && qrContainerRef.current) {
            qrContainerRef.current.innerHTML = qrSvg;
            // 给生成的 SVG 添加样式
            const svg = qrContainerRef.current.querySelector('svg');
            if (svg) {
                svg.style.width = '100%';
                svg.style.height = '100%';
            }
        }
    }, [qrSvg]);

    const handleClose = () => {
        stopPolling();
        onOpenChange(false);
    };

    return (
        <Dialog open={open} onOpenChange={handleClose}>
            <DialogContent className="sm:max-w-md">
                <DialogHeader>
                    <DialogTitle>扫描二维码登录微信</DialogTitle>
                    <DialogDescription>
                        请使用微信扫描二维码完成登录
                    </DialogDescription>
                </DialogHeader>
                <div className="flex flex-col items-center justify-center space-y-4">
                    <div className="relative w-64 h-64 bg-white rounded-lg overflow-hidden flex items-center justify-center shadow-sm">
                        {isLoading ? (
                            <Loader2 className="h-8 w-8 animate-spin text-gray-400" />
                        ) : qrSvg ? (
                            <div
                                ref={qrContainerRef}
                                className="w-full h-full flex items-center justify-center p-2"
                            />
                        ) : (
                            <div className="text-center p-4">
                                <p className="text-sm text-red-500 mb-2">二维码生成失败</p>
                                <Button
                                    size="sm"
                                    onClick={initQRCode}
                                    variant="outline"
                                >
                                    重新加载
                                </Button>
                            </div>
                        )}
                    </div>
                    {isPolling && (
                        <div className="flex items-center gap-2 text-sm text-muted-foreground">
                            <Loader2 className="h-4 w-4 animate-spin" />
                            <span>等待扫码...</span>
                        </div>
                    )}
                    <p className="text-xs text-muted-foreground text-center">
                        请使用微信扫描二维码完成登录
                    </p>
                    <Button
                        variant="ghost"
                        size="sm"
                        onClick={handleClose}
                        className="mt-2"
                    >
                        取消
                    </Button>
                </div>
            </DialogContent>
        </Dialog>
    );
};