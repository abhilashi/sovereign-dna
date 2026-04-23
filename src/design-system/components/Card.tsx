import React, { type ReactNode } from 'react';
import { motion } from 'framer-motion';

interface CardProps {
  children: ReactNode;
  className?: string;
  onClick?: () => void;
  onKeyDown?: (e: React.KeyboardEvent<HTMLDivElement>) => void;
}

export default function Card({ children, className = '', onClick, onKeyDown }: CardProps) {
  const interactive = !!onClick;

  return (
    <motion.div
      className={`bg-surface border border-border rounded-sm p-5 ${interactive ? 'cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent' : ''} ${className}`}
      onClick={onClick}
      role={interactive ? "button" : undefined}
      tabIndex={interactive ? 0 : undefined}
      onKeyDown={interactive ? (e: React.KeyboardEvent<HTMLDivElement>) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onClick?.();
        }
        onKeyDown?.(e);
      } : onKeyDown}
      whileHover={interactive ? { scale: 1.005, boxShadow: '0 1px 3px rgba(0,0,0,0.06)' } : undefined}
      transition={{ duration: 0.15, ease: 'easeOut' }}
    >
      {children}
    </motion.div>
  );
}
