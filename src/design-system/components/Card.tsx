import { type ReactNode } from 'react';
import { motion } from 'framer-motion';

interface CardProps {
  children: ReactNode;
  className?: string;
  onClick?: () => void;
}

export default function Card({ children, className = '', onClick }: CardProps) {
  const interactive = !!onClick;

  return (
    <motion.div
      className={`bg-surface border border-border rounded-sm p-5 ${interactive ? 'cursor-pointer' : ''} ${className}`}
      onClick={onClick}
      whileHover={interactive ? { scale: 1.005, boxShadow: '0 1px 3px rgba(0,0,0,0.06)' } : undefined}
      transition={{ duration: 0.15, ease: 'easeOut' }}
    >
      {children}
    </motion.div>
  );
}
