import { CATEGORY_COLORS } from "./StatusBadge";

export default function CategoryBadge({ category }) {
  const cat = CATEGORY_COLORS[category] || CATEGORY_COLORS.unknown;
  return (
    <span className={`inline-flex items-center px-2 py-0.5 rounded text-xs font-medium ${cat.bg} ${cat.text}`}>
      {cat.label}
    </span>
  );
}
