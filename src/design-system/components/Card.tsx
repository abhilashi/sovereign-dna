import { type ReactNode, type KeyboardEvent } from 'react';
import { motion, type HTMLMotionProps } from 'framer-motion';

interface CardProps {
  children: ReactNode;
  className?: string;
  onClick?: () => void;
}

export default function Card({ children, className = '', onClick }: CardProps) {
  const interactive = !!onClick;

  const handleKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!interactive || !onClick) return;
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault(); // Prevent page scroll on Space
      onClick();
    }
  };

  const interactiveProps: HTMLMotionProps<"div"> = interactive ? {
    role: "button",
    tabIndex: 0,
    onKeyDown: handleKeyDown,
    className: `bg-surface border border-border rounded-sm p-5 cursor-pointer focus-visible:ring-1 focus-visible:ring-accent focus:outline-none ${className}`
  } : {
    className: `bg-surface border border-border rounded-sm p-5 ${className}`
  };

  return (
    <motion.div
      {...interactiveProps}
      onClick={onClick}
      whileHover={interactive ? { scale: 1.005, boxShadow: '0 1px 3px rgba(0,0,0,0.06)' } : undefined}
      transition={{ duration: 0.15, ease: 'easeOut' }}
    >
      {children}
    </motion.div>
  );
}
