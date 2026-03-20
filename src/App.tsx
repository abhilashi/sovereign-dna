import { BrowserRouter, Routes, Route } from 'react-router-dom';
import DashboardLayout from './design-system/layouts/DashboardLayout';
import Dashboard from './pages/Dashboard';
import Import from './pages/Import';
import HealthRisks from './pages/HealthRisks';
import Pharmacogenomics from './pages/Pharmacogenomics';
import Traits from './pages/Traits';
import Ancestry from './pages/Ancestry';
import CarrierStatus from './pages/CarrierStatus';
import ResearchFeed from './pages/ResearchFeed';
import Explorer from './pages/Explorer';
import Reports from './pages/Reports';
import GenomeMap from './pages/GenomeMap';
import Settings from './pages/Settings';
import ResearchWorkbench from './pages/ResearchWorkbench';

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route element={<DashboardLayout />}>
          <Route path="/" element={<ResearchWorkbench />} />
          <Route path="/dashboard" element={<Dashboard />} />
          <Route path="/map" element={<GenomeMap />} />
          <Route path="/import" element={<Import />} />
          <Route path="/health" element={<HealthRisks />} />
          <Route path="/pharma" element={<Pharmacogenomics />} />
          <Route path="/traits" element={<Traits />} />
          <Route path="/ancestry" element={<Ancestry />} />
          <Route path="/carrier" element={<CarrierStatus />} />
          <Route path="/research" element={<ResearchFeed />} />
          <Route path="/explorer" element={<Explorer />} />
          <Route path="/reports" element={<Reports />} />
          <Route path="/settings" element={<Settings />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}

export default App;
