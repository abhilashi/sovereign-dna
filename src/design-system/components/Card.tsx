import { type ReactNode } from 'react';
import { motion } from 'framer-motion';

interface CardProps {
  children: ReactNode;
  className?: string;
  onClick?: () => void;
}

export default function Card({ children, className = '', onClick }: CardProps) {
  const interactive = !!onClick;

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (!interactive) return;
    if (e.key === 'Enter' || e.key === ' ') {
      if (e.key === ' ') {
        e.preventDefault(); // Prevent scrolling when pressing space
      }
      onClick?.();
    }
  };

  return (
    <motion.div
      className={`bg-surface border border-border rounded-sm p-5 ${interactive ? 'cursor-pointer focus-visible:outline-none focus-visible:border-accent focus-visible:ring-1 focus-visible:ring-accent' : ''} ${className}`}
      onClick={onClick}
      role={interactive ? 'button' : undefined}
      tabIndex={interactive ? 0 : undefined}
      onKeyDown={interactive ? handleKeyDown : undefined}
      whileHover={interactive ? { scale: 1.005, boxShadow: '0 1px 3px rgba(0,0,0,0.06)' } : undefined}
      transition={{ duration: 0.15, ease: 'easeOut' }}
    >
      {children}
    </motion.div>
  );
}
