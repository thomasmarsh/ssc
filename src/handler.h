#ifndef SSC_HANDLER_H
#define SSC_HANDLER_H

#include <SDL/SDL.h>
#include <cstring>

// ---------------------------------------------------------------------------
//
// EVENT HANDLER
//
// Dispatches keyboard, mouse and joystick events to the game subsystems.
//
// ---------------------------------------------------------------------------

class Controller;

class EventHandler
{
private:
	inline void mouseMove() { mMouseMove = true; }
	inline void clearMouseMove() { mMouseMove = false; }
	bool mMouseMove;

	bool mKeymap[SDLK_LAST];
	bool mMouseButton[4];
	int x, y;
protected:
	Controller *mController;
public:
	EventHandler()
	{
		for (int c = 0; c < SDLK_LAST; c++)
			mKeymap[c] = false;
		for (int i = 0; i < 4; i++)
			mMouseButton[i] = false;
		mMouseMove = false;
	}
	virtual ~EventHandler() {}

	void setController(Controller *c) { mController = c; }

	// controller interface
	virtual inline void pressKey(SDLKey c) { mKeymap[c] = true; }
	virtual inline void releaseKey(SDLKey c) { mKeymap[c] = false; }
	virtual inline void mouseMotion(int mx, int my) { x = mx, y = my; mouseMove(); }
	virtual inline void pressMouseButton(int b) { mMouseButton[b] = true; }
	virtual inline void releaseMouseButton(int b) { mMouseButton[b] = false; }

	virtual void process(double dt) = 0;

protected:
	// client (subclass) interface
	inline bool mouseMotion() { bool m = mMouseMove; mMouseMove = false; return m; }
	inline bool keyPressed(SDLKey c) { return mKeymap[c]; }

	inline int mouseX() { return x; }
	inline int mouseY() { return y; }
	inline bool mouseButton(int b) { return mMouseButton[b]; }
};


// ---------------------------------------------------------------------------
// 
// PLAY HANDLER
//
// Handles events during gameplay
//
// ---------------------------------------------------------------------------

class PlayHandler : public EventHandler
{
public:
	PlayHandler();
	void process(double dt);
};


// ---------------------------------------------------------------------------
// 
// MENU HANDLER
//
// Handles events while in the menu system
//
// ---------------------------------------------------------------------------

class GameMenu;

class MenuHandler : public EventHandler
{
public:
	MenuHandler();
	void process(double dt);
	void pressKey(SDLKey c);
	void setMenu(GameMenu *m) { mGameMenu = m; }
private:
	GameMenu *mGameMenu;
};

#endif // SSC_HANDLER_H
