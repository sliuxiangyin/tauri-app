import { MoreVerticalIcon } from "lucide-react"

import {
    Item,
    ItemActions,
    ItemContent,
    ItemTitle,
} from "@/components/ui/item"
import { McpServeConfig } from "@/lib/mcp-serve-api"
import { NavigationMenu, NavigationMenuList, NavigationMenuItem, NavigationMenuTrigger, NavigationMenuContent, NavigationMenuLink } from "../ui/navigation-menu"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"

export function ItemLink({ cfg, onSelect }: { cfg: McpServeConfig, onSelect: (id: number) => void }) {
     
    return (
        <div className="flex w-full max-w-md flex-col mb-2">
            <Item variant="outline" asChild >
                <a role="button" className="cursor-pointer" onClick={() => onSelect(cfg.id)}>

                    <ItemContent>
                        <ItemTitle>
                            <span className={`inline-block w-2 h-2 rounded-full mr-2 ${cfg.state ? 'bg-green-500' : 'bg-red-500'}`} />
                            {cfg.name}
                        </ItemTitle>
                        <NavigationMenu>
                            <NavigationMenuList>
                                <NavigationMenuItem>
                                    <NavigationMenuTrigger>tools ({cfg.tools.length})</NavigationMenuTrigger>
                                    <NavigationMenuContent>
                                        <ul className="w-56 max-h-64 overflow-y-auto">
                                            {cfg.tools.map((tool) => (
                                                <ToolListItem
                                                    key={tool.name}
                                                    title={tool.name}
                                                >
                                                    {tool.description || "No description"}
                                                </ToolListItem>
                                            ))}
                                            {cfg.tools.length === 0 && (
                                                <li className="px-4 py-2 text-sm text-muted-foreground">
                                                    No tools available
                                                </li>
                                            )}
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

function ToolListItem({
    title,
    children,
    ...props
}: React.ComponentPropsWithoutRef<"li">) {
    return (
        <li {...props}>
            <NavigationMenuLink asChild className="items-left">
                <Tooltip>
                    <TooltipTrigger asChild>
                        <div className="flex flex-col gap-1 px-3 py-2 text-left text-sm hover:bg-accent rounded-sm">
                            <div className="leading-none font-medium">{title}</div>
                            <div className="line-clamp-2 text-muted-foreground">{children}</div>
                        </div>
                    </TooltipTrigger>
                    <TooltipContent side="right" className="max-w-xs  text-white">
                        <div className="flex flex-col gap-1">
                            <p className="font-medium">{title}</p>
                            <p className="text-muted-foreground text-white">{children}</p>
                        </div>
                    </TooltipContent>
                </Tooltip>
            </NavigationMenuLink>
        </li>
    )
}