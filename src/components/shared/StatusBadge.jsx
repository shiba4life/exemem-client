export const CATEGORY_COLORS = {
  personal_data: { bg: "bg-blue-100", text: "text-blue-700", label: "Personal Data" },
  media: { bg: "bg-purple-100", text: "text-purple-700", label: "Media" },
  config: { bg: "bg-gray-100", text: "text-gray-600", label: "Config" },
  website_scaffolding: { bg: "bg-orange-100", text: "text-orange-700", label: "Scaffolding" },
  work: { bg: "bg-teal-100", text: "text-teal-700", label: "Work" },
  unknown: { bg: "bg-gray-100", text: "text-gray-500", label: "Unknown" },
};

export default function StatusBadge({ phase, watching }) {
  const labels = {
    settings: "Setup",
    scanning: "Scanning",
    review: "Review",
    ingesting: "Ingesting",
    watching: watching ? "Watching" : "Paused",
  };
  const colors = {
    settings: "bg-gray-100 text-gray-600",
    scanning: "bg-yellow-100 text-yellow-700",
    review: "bg-blue-100 text-blue-700",
    ingesting: "bg-yellow-100 text-yellow-700",
    watching: watching ? "bg-green-100 text-green-700" : "bg-gray-100 text-gray-600",
  };

  return (
    <span className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium ${colors[phase]}`}>
      <span className={`w-2 h-2 rounded-full ${
        phase === "watching" && watching ? "bg-green-500 animate-pulse" :
        phase === "scanning" || phase === "ingesting" ? "bg-yellow-500 animate-pulse" :
        "bg-gray-400"
      }`} />
      {labels[phase]}
    </span>
  );
}
