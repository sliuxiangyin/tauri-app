import { MoreVerticalIcon } from "lucide-react"

import {
    Item,
    ItemActions,
    ItemContent,
    ItemTitle,
} from "@/components/ui/item"
import type { McpServeConfig } from "@/stores/useMcpStore"
import { NavigationMenu, NavigationMenuList, NavigationMenuItem, NavigationMenuTrigger, NavigationMenuContent } from "../ui/navigation-menu"

export function ItemLink({ cfg, onSelect }: { cfg: McpServeConfig, onSelect: (id: string) => void }) {
    // 判断是否已连接
    const isConnected = cfg.state === 'Connected'
    
    return (
        <div className="flex w-full max-w-md mb-2 flex-col">
            <Item variant="outline" asChild >
                <a role="button" className="cursor-pointer" onClick={() => onSelect(cfg.id.toString())}>

                    <ItemContent>
                        <ItemTitle>
                            <span className={`inline-block w-2 h-2 rounded-full mr-2 ${isConnected ? 'bg-green-500' : 'bg-red-500'}`} />
                            {cfg.name}
                        </ItemTitle>
                        <NavigationMenu>
                            <NavigationMenuList>
                                <NavigationMenuItem>
                                    <NavigationMenuTrigger>tools ({cfg.tool_count})</NavigationMenuTrigger>
                                    <NavigationMenuContent>
                                        <ul className="w-56 max-h-64 overflow-y-auto">
                                            <li className="px-4 py-2 text-sm text-muted-foreground">
                                                {cfg.tool_count > 0 ? `${cfg.tool_count} 个工具` : '暂无工具'}
                                            </li>
                                        </ul>
                                    </NavigationMenuContent>
                                </NavigationMenuItem>
                            </NavigationMenuList>
                        </NavigationMenu>
                    </ItemContent>
                    <ItemActions>
                        <MoreVerticalIcon className="size-4" />
                    </ItemActions>
                </a>
            </Item>
        </div>
    )
}