// QRCodeLoginDialog.tsx
"use client";

import React, { useState, useEffect, useRef } from "react";
import { Loader2, RefreshCw } from "lucide-react";
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { toast } from "sonner";
import { startLoginStream, WechatLoginEvent, LoginError } from "@/lib/api/wechat";
import encodeQR from "qr";
import { UnlistenFn } from "@tauri-apps/api/event";

interface QRCodeLoginDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    onLoginSuccess?: () => void | Promise<void>;
}

type LoginStatus = 'idle' | 'generating' | 'waiting_scan' | 'scanned' | 'confirmed' | 'success' | 'expired' | 'failed';

export const QRCodeLoginDialog: React.FC<QRCodeLoginDialogProps> = ({
    open,
    onOpenChange,
    onLoginSuccess,
}) => {
    const [status, setStatus] = useState<LoginStatus>('idle');
    const [qrSvg, setQrSvg] = useState<string>("");
    const [errorMessage, setErrorMessage] = useState<string>("");
    const [retryInfo, setRetryInfo] = useState<{ current: number; max: number } | null>(null);

    const unlistenRef = useRef<UnlistenFn | null>(null);
    const qrContainerRef = useRef<HTMLDivElement>(null);
    const accountIdRef = useRef<string>("temp_account");
    // 清理函数：取消事件监听并重置状态
    const cleanup = async () => {
        if (unlistenRef.current) {
            await unlistenRef.current();
            unlistenRef.current = null;
        }
        setQrSvg("");
        setStatus('idle');
        setErrorMessage("");
        setRetryInfo(null);
    };

    // 处理登录事件
    const handleEvent = (event: WechatLoginEvent) => {
        switch (event.event_type) {
            case 'qr_generated': {
                const qrData = event.data.qrDataUrl;
                // 将URL转换为二维码SVG
                const svgString = encodeQR(qrData, 'svg', { scale: 8 });
                setQrSvg(svgString);
                setStatus('waiting_scan');
                setErrorMessage("");
                break;
            }
            case 'scanned':
                setStatus('scanned');
                break;
            case 'confirmed':
                setStatus('confirmed');
                break;
            case 'login_success':
                setStatus('success');
                toast.success("微信登录成功！", {
                    description: event.data.message,
                    duration: 3000,
                });
                setTimeout(async () => {
                    if (onLoginSuccess) {
                        await onLoginSuccess();
                    }
                    onOpenChange(false);
                }, 1500);
                break;
            case 'login_failed':
                setStatus('failed');
                setErrorMessage(event.data.message);
                toast.error("登录失败", {
                    description: event.data.message,
                });
                 onOpenChange(false);
                break;
            case 'qr_expired':
                setStatus('expired');
                setErrorMessage(event.data.message);
                setRetryInfo({
                    current: event.data.retryCount,
                    max: event.data.maxRetries,
                });
                break;
            case 'error':
                setStatus('failed');
                setErrorMessage(event.data.message);
                toast.error("登录错误", {
                    description: event.data.message,
                });
                break;
        }
    };

    // 处理错误回调
    const handleError = (error: LoginError) => {
        console.error("Login error:", error);
        setStatus('failed');
        setErrorMessage(error.message);
        toast.error("登录异常", {
            description: error.message,
        });
    };

    // 启动登录流
    const startLogin = async () => {
        setStatus('generating');
        setErrorMessage("");

        try {
            const unlisten = await startLoginStream(
                accountIdRef.current,
                handleEvent,
                handleError
            );
            unlistenRef.current = unlisten;
            console.log("unlistenRef.current",unlistenRef.current);
        } catch (error) {
            console.error("Failed to start login stream:", error);
            setStatus('failed');
            setErrorMessage(error instanceof Error ? error.message : "启动登录失败");
            toast.error("启动登录失败", {
                description: error instanceof Error ? error.message : "请重试",
            });
        }
    };

    const handleRetry = () => {
        startLogin();
    };

    // open=true 启动登录流，open=false 清理资源
    useEffect(() => {
        if (open) {
            startLogin();
        } else {
            cleanup();
        }
    }, [open]);

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
        cleanup();
        onOpenChange(false);
    };

    // 根据状态渲染内容
    const renderContent = () => {
        if (status === 'generating') {
            return (
                <div className="flex items-center justify-center h-64">
                    <Loader2 className="h-8 w-8 animate-spin text-gray-400" />
                    <span className="ml-2 text-sm text-muted-foreground">正在生成二维码...</span>
                </div>
            );
        }

        if (status === 'waiting_scan' || status === 'scanned' || status === 'confirmed') {
            return (
                <>
                    <div className="w-64 h-64 bg-white rounded-lg overflow-hidden flex items-center justify-center shadow-sm">
                        <div
                            ref={qrContainerRef}
                            className="w-full h-full flex items-center justify-center p-2"
                        />
                    </div>
                    <div className="flex items-center gap-2 text-sm text-muted-foreground">
                        {status === 'waiting_scan' && (
                            <>
                                <Loader2 className="h-4 w-4 animate-spin" />
                                <span>等待扫码...</span>
                            </>
                        )}
                        {status === 'scanned' && (
                            <span className="text-blue-500">✓ 已扫码，请在手机确认</span>
                        )}
                        {status === 'confirmed' && (
                            <span className="text-green-500">✓ 确认中...</span>
                        )}
                    </div>
                </>
            );
        }

        if (status === 'expired') {
            return (
                <>
                    <div className="w-64 h-64 bg-gray-100 rounded-lg flex items-center justify-center">
                        <RefreshCw className="h-16 w-16 text-gray-400" />
                    </div>
                    <div className="text-center space-y-2">
                        <p className="text-sm text-orange-500">{errorMessage}</p>
                        {retryInfo && retryInfo.current < retryInfo.max && (
                            <p className="text-xs text-muted-foreground">
                                将在 {3 - retryInfo.current} 秒后自动刷新
                            </p>
                        )}
                    </div>
                    <Button size="sm" variant="outline" onClick={handleRetry}>
                        <RefreshCw className="h-4 w-4 mr-2" />
                        重新获取二维码
                    </Button>
                </>
            );
        }

        if (status === 'failed') {
            return (
                <>
                    <div className="w-64 h-64 bg-gray-100 rounded-lg flex items-center justify-center">
                        <p className="text-sm text-red-500 text-center px-4">{errorMessage}</p>
                    </div>
                    <Button size="sm" variant="outline" onClick={handleRetry}>
                        <RefreshCw className="h-4 w-4 mr-2" />
                        重试
                    </Button>
                </>
            );
        }

        return null;
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
                <div className="flex flex-col items-center justify-center space-y-4 py-4">
                    {renderContent()}
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