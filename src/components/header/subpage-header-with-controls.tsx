
import {BaseHeader} from "@/components/header/base-header.tsx";

// ========== 主组件 ==========
export const SubpageHeaderWithControls = ({ title }: { title: string }) => {

    return (
        <BaseHeader
            title={title}
            titleCentered={true}
        >
        </BaseHeader>
    );
};
