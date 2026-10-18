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
    if (e.key === 'Enter') {
      onClick();
    } else if (e.key === ' ') {
      e.preventDefault();
      onClick();
    }
  };

  const interactiveProps: Partial<HTMLMotionProps<"div">> = interactive
    ? {
        role: 'button',
        tabIndex: 0,
        onClick,
        onKeyDown: handleKeyDown,
        whileHover: { scale: 1.005, boxShadow: '0 1px 3px rgba(0,0,0,0.06)' },
      }
    : {};

  return (
    <motion.div
      className={`bg-surface border border-border rounded-sm p-5 ${
        interactive
          ? 'cursor-pointer focus-visible:ring-1 focus-visible:ring-accent focus:outline-none'
          : ''
      } ${className}`}
      transition={{ duration: 0.15, ease: 'easeOut' }}
      {...interactiveProps}
    >
      {children}
    </motion.div>
  );
}
