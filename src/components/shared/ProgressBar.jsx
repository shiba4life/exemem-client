export default function ProgressBar({ percent, status }) {
  const color = status === "error" ? "bg-red-500" :
    status === "done" ? "bg-green-500" :
    "bg-blue-500";
  return (
    <div className="w-full bg-gray-200 rounded-full h-2">
      <div className={`${color} h-2 rounded-full transition-all duration-300`} style={{ width: `${Math.min(100, percent)}%` }} />
    </div>
  );
}
