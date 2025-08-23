import React, { useState, useEffect } from 'react';
import notificationService from './services/notificationService';
import './App.css';

function App() {
  const [notificationPermission, setNotificationPermission] = useState('default');
  const [isSubscribed, setIsSubscribed] = useState(false);
  const [isStandalone, setIsStandalone] = useState(false);

  useEffect(() => {
    const initializeApp = async () => {
      // Initialize service worker
      await notificationService.initialize();
      
      // Check initial permission status
      setNotificationPermission(notificationService.getPermissionStatus());
      
      // Check if running in standalone mode (iOS PWA)
      setIsStandalone(notificationService.isStandalone());
    };

    initializeApp();
  }, []);

  const handleRequestPermission = async () => {
    const granted = await notificationService.requestPermission();
    setNotificationPermission(notificationService.getPermissionStatus());
    
    if (granted) {
      try {
        await notificationService.subscribeToPush();
        setIsSubscribed(true);
      } catch (error) {
        console.error('Failed to subscribe to push notifications:', error);
      }
    }
  };

  const handleTestNotification = () => {
    notificationService.showLocalNotification('Test Notification', {
      body: 'This is a test notification from Boomerang!',
      tag: 'test-notification'
    });
  };

  const handleScheduleTest = () => {
    notificationService.scheduleLocalNotification(
      'Scheduled Notification',
      'This notification was scheduled 5 seconds ago!',
      5000
    );
  };

  const renderInstallPrompt = () => {
    if (notificationService.canInstall()) {
      return (
        <div className="install-prompt">
          <h3>Install Boomerang</h3>
          <p>For the best experience, add Boomerang to your home screen:</p>
          <ol>
            <li>Tap the Share button in Safari</li>
            <li>Scroll down and tap "Add to Home Screen"</li>
            <li>Tap "Add" in the top right corner</li>
          </ol>
        </div>
      );
    }
    return null;
  };

  return (
    <div className="App">
      <header className="App-header">
        <h1>Boomerang PWA</h1>
        <p>Scheduled LLM Tool Execution with iOS Notifications</p>
        
        {isStandalone && (
          <div className="standalone-indicator">
            ✅ Running as PWA
          </div>
        )}
      </header>

      <main className="App-main">
        {renderInstallPrompt()}
        
        <section className="notification-section">
          <h2>Notification Setup</h2>
          
          <div className="status-info">
            <p><strong>Permission Status:</strong> {notificationPermission}</p>
            <p><strong>Push Subscription:</strong> {isSubscribed ? 'Active' : 'Inactive'}</p>
            <p><strong>Standalone Mode:</strong> {isStandalone ? 'Yes' : 'No'}</p>
          </div>

          <div className="button-group">
            {notificationPermission !== 'granted' && (
              <button 
                onClick={handleRequestPermission}
                className="primary-button"
              >
                Enable Notifications
              </button>
            )}
            
            {notificationPermission === 'granted' && (
              <>
                <button 
                  onClick={handleTestNotification}
                  className="secondary-button"
                >
                  Test Notification
                </button>
                
                <button 
                  onClick={handleScheduleTest}
                  className="secondary-button"
                >
                  Schedule Test (5s)
                </button>
              </>
            )}
          </div>
        </section>

        <section className="schedule-section">
          <h2>Schedule Management</h2>
          <p>Schedule creation interface will be implemented here.</p>
          
          <div className="placeholder-content">
            <div className="schedule-item">
              <h3>Example Schedule</h3>
              <p>"Every morning, M-F, check my emails and notify me with a summary"</p>
              <span className="status active">Active</span>
            </div>
          </div>
        </section>
      </main>
    </div>
  );
}

export default App;